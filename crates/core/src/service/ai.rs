//! AI 服务接入：OpenAI 兼容协议（智谱 GLM 默认 / DeepSeek / Ollama 等均可）。
//! 设计原则：
//! - 按需供给上下文（单篇只发单篇文本），配 token 预算与截断保护，避免长上下文污染分析
//! - API Key 仅存本地 SQLite，不进 git、不进前端
//! - 输出永远走"预览→确认→入库"，人工把关

use crate::db::CoreState;
use crate::error::{CoreError, CoreResult};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};

pub const AI_CONFIG_KEY: &str = "ai_config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    /// OpenAI 兼容 Base URL（如 https://open.bigmodel.cn/api/paas/v4）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 单次请求上下文预算（token 估算），超出截断，防上下文污染
    pub max_context_tokens: usize,
    /// 生成温度：文献分析要稳定，默认低温
    pub temperature: f64,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            api_key: String::new(),
            model: "glm-5.3-flash".into(),
            max_context_tokens: 120_000,
            temperature: 0.3,
        }
    }
}

pub fn get_config(state: &CoreState) -> AiConfig {
    let inner = state.inner.lock().unwrap();
    let text: Option<String> = inner
        .conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![AI_CONFIG_KEY],
            |r| r.get(0),
        )
        .ok();
    text.and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn set_config(state: &CoreState, cfg: &AiConfig) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    inner.conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![AI_CONFIG_KEY, serde_json::to_string(cfg)?],
    )?;
    Ok(())
}

/// 粗略 token 估算：中文约 1 字/token，英文约 4 字符/token，取保守混合估算。
pub fn estimate_tokens(text: &str) -> usize {
    let cjk = text.chars().filter(|c| (*c as u32) >= 0x4E00 && (*c as u32) <= 0x9FFF).count();
    let other = text.chars().count() - cjk;
    cjk + other / 4 + 1
}

/// 按预算截断长文：保留头部为主 + 尾部收尾（结论常在尾部），中段标注截断。
pub fn truncate_to_budget(text: &str, max_tokens: usize) -> String {
    let total = estimate_tokens(text);
    if total <= max_tokens {
        return text.to_string();
    }
    // 头 72% + 截断标记 + 尾 18%（预留标记开销）
    let head_tokens = max_tokens * 72 / 100;
    let tail_tokens = max_tokens * 18 / 100;
    let chars: Vec<char> = text.chars().collect();
    // 由 token 反推字符：用与估算一致的比例近似
    let head_chars = proportion_chars(&chars, head_tokens, text);
    let tail_chars = proportion_chars(&chars, tail_tokens, text);
    let mut out: String = chars[..head_chars].iter().collect();
    out.push_str("\n\n[注意：原文过长，此处已截断中段内容以控制上下文。已提供开头与结尾部分。]\n\n");
    out.extend(chars[chars.len() - tail_chars..].iter());
    out
}

fn proportion_chars(chars: &[char], tokens: usize, full: &str) -> usize {
    let cjk = full.chars().filter(|c| (*c as u32) >= 0x4E00 && (*c as u32) <= 0x9FFF).count();
    let cjk_ratio = cjk as f64 / full.chars().count().max(1) as f64;
    // 每 token 平均字符数（cjk≈1，其他≈4）
    let per_token = cjk_ratio + (1.0 - cjk_ratio) * 4.0;
    (((tokens as f64) * per_token) as usize).min(chars.len())
}

/// 文献分析系统提示词：固化模板与规则（需求文档 5.5），保证输出可被解析器处理。
pub fn analysis_system_prompt() -> String {
    format!(
        "你是一名严谨的学术文献分析助手。请阅读用户提供的论文全文，严格按照以下 Markdown 模板输出分析。\n\n{}\n\n硬性规则：\n1. 保留 YAML 字段名与八个二级标题，不增删改名，不要增加模板外的解释，不要把整份输出包在代码块中。\n2. 仅使用论文能够支持的内容；无法确认的信息标注「无法确认」，不要猜测作者、DOI、数据、引文或页码。\n3. 原文明确未报告的内容写「原文未报告」；不得把空白当作否定结论或数值零。\n4. 重要发现尽可能附准确原文摘录（引用块）和 PDF 实际页码链接，格式为 [PDF 第 N 页](#pdf-page-N)，N 从 PDF 文件第 1 页开始计数（不是期刊印刷页码）；不能确认页码时省略链接。\n5. 区分论文作者报告的局限和你自己的疑问；你的疑问不要写进「局限性」。\n6. 控制篇幅：每个栏目 3-8 句或等量条目，聚焦核心信息。",
        include_str!("../../../../文献AI分析模板.md")
    )
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
}

/// 流式生成：逐块回调 on_chunk(delta_text)，返回完整文本。
/// OpenAI 兼容 SSE：data: {"choices":[{"delta":{"content":"..."}}]}
pub fn generate_stream(
    cfg: &AiConfig,
    system: &str,
    user: &str,
    on_chunk: &mut dyn FnMut(&str),
) -> CoreResult<String> {
    if cfg.api_key.trim().is_empty() {
        return Err(CoreError::Msg("未配置 AI API Key：请到 设置与备份 → AI 服务 填写".into()));
    }
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = ChatRequest {
        model: &cfg.model,
        messages: vec![
            ChatMessage { role: "system", content: system.to_string() },
            ChatMessage { role: "user", content: user.to_string() },
        ],
        temperature: Some(cfg.temperature),
        stream: true,
    };
    let client = ureq::AgentBuilder::new()
        .timeout_read(std::time::Duration::from_secs(300))
        .build();
    let resp = client
        .post(&url)
        .set("Authorization", &format!("Bearer {}", cfg.api_key))
        .set("Content-Type", "application/json")
        .send_json(serde_json::to_value(&body)?)
        .map_err(|e| CoreError::Msg(format!("AI 请求失败：{e}")))?;

    let reader = BufReader::new(resp.into_reader());
    let mut full = String::new();
    for line in reader.lines() {
        let line = line.map_err(|e| CoreError::Msg(format!("AI 流读取失败：{e}")))?;
        let data = match line.strip_prefix("data:") {
            Some(d) => d.trim(),
            None => continue,
        };
        if data == "[DONE]" {
            break;
        }
        let v: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue, // 心跳/非 JSON 行跳过
        };
        let delta = v
            .pointer("/choices/0/delta/content")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        if !delta.is_empty() {
            full.push_str(delta);
            on_chunk(delta);
        }
    }
    if full.trim().is_empty() {
        return Err(CoreError::Msg("AI 返回为空：请检查模型名与 API Key".into()));
    }
    Ok(full)
}

/// 组会汇报大纲系统提示词：输入多篇文献的结构化分析，输出 Markdown 大纲。
/// 大纲中留有 [编辑位] 让用户填自己本周的工作描述。
pub fn outline_system_prompt() -> String {
    let mut rules = String::new();
    rules.push_str("你是一名学术研究组会汇报助手。根据用户提供的多篇文献分析数据，生成一份可直接用于组会汇报的 Markdown 大纲。

");
    rules.push_str("硬性规则：

");
    rules.push_str("1. 按以下固定结构输出（二级标题），可根据内容微调标题但保持逻辑顺序：
");
    rules.push_str("   ## 研究背景
   ## 文献综述（按主题分组，非逐篇罗列）
   ## 方法对比
   ## 主要发现
   ## 共识与分歧
   ## 研究空白与下一步
   ## 个人工作总结
   ## 讨论与建议

");
    rules.push_str("2. 「个人工作总结」是用户的编辑位：AI 只写一行占位提示（如：[请描述本周实验进展/阅读心得/遇到的问题]），不替用户编造内容。

");
    rules.push_str("3. 每条要点后用 `（来源：文献编号）` 标注来源，编号在末尾参考文献列表定义。

");
    rules.push_str("4. 文献综述按主题/方法分组讨论，不要逐篇流水账。

");
    rules.push_str("5. 控制在 15-25 页当量的大纲（每个二级标题下 3-8 条要点）。

");
    rules.push_str("6. 末尾附「参考文献」列表：编号 + 标题 + 作者 + 年份。

");
    rules.push_str("7. 用 Markdown 输出，不要包在代码块中。");
    rules
}

pub fn build_outline_prompt(papers: &[(String, String)], extra: &str) -> String {
    let mut out = String::from("请根据以下文献分析数据生成组会汇报大纲。

");
    for (i, (title, analysis_md)) in papers.iter().enumerate() {
        out.push_str(&format!("
---
### 文献 [{}]：{}
{}
", i + 1, title, analysis_md));
    }
    out.push_str("
---
");
    if !extra.trim().is_empty() {
        out.push_str(&format!("
用户补充说明：{}
", extra.trim()));
    }
    out.push_str("
请生成完整的组会汇报大纲。");
    out
}

/// 测试连接：发一个极小请求验证配置可用。
pub fn test_connection(cfg: &AiConfig) -> CoreResult<String> {
    let mut out = String::new();
    let mut sink = |c: &str| out.push_str(c);
    // 用最短提问，忽略预算截断
    let mut cfg2 = cfg.clone();
    cfg2.temperature = 0.0;
    generate_stream(&cfg2, "You are a test.", "回复两个字：成功", &mut sink)?;
    Ok(out)
}
