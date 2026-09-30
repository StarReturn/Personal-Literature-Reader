# -*- coding: utf-8 -*-
"""笔记汇总导出端到端测试。"""
import json
import urllib.request

BASE = "http://127.0.0.1:8787"


def req(p, m="GET", b=None):
    data = json.dumps(b).encode() if b is not None else None
    r = urllib.request.Request(BASE + p, data=data, method=m,
                               headers={"Content-Type": "application/json"} if data else {})
    return json.loads(urllib.request.urlopen(r, timeout=30).read().decode())


papers = req("/api/papers")
ids = [p["id"] for p in papers]
print("papers:", len(ids))

req(f"/api/papers/{ids[0]}/notes", "PUT", {
    "content": "## 核心观点\n\n- 注意力替代循环是主线\n- 效率对比要注意条件\n\n**可入 PPT**：性能提升 3.5pp 的图"})
req(f"/api/papers/{ids[1]}/notes", "PUT", {
    "content": "- 稀疏化的显存收益最实用\n- 协议不统一是综述痛点"})

r = urllib.request.urlopen(BASE + "/api/export-notes", timeout=30)
text = r.read().decode("utf-8")
print("status:", r.status)
checks = {
    "标题": "# 文献笔记汇总" in text,
    "用途提示(PPT大纲)": "PPT 大纲" in text,
    "笔记1内容": "可入 PPT" in text,
    "笔记2内容": "显存收益" in text,
    "篇数统计(2)": "共 2 篇文献的个人笔记" in text,
    "不含AI分析原文": "一句话总结" not in text,
}
for k, v in checks.items():
    print(("PASS " if v else "FAIL ") + k)
print("--- 头部 380 字 ---")
print(text[:380])
