# -*- coding: utf-8 -*-
"""API 级端到端验收：对照需求文档第 10 节的验收标准（后端部分）。"""
import io
import json
import os
import sys
import urllib.parse
import urllib.request
import uuid

BASE = "http://127.0.0.1:8787"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLES = os.path.join(ROOT, "samples")
PASS = 0
FAIL = 0


def check(name: str, cond: bool, detail: str = ""):
    global PASS, FAIL
    if cond:
        PASS += 1
        print(f"  PASS {name}")
    else:
        FAIL += 1
        print(f"  FAIL {name}  {detail}")


def post_multipart(path: str, files: dict, fields: dict = None):
    boundary = uuid.uuid4().hex
    body = io.BytesIO()
    fields = fields or {}
    for k, v in fields.items():
        body.write(f"--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n".encode())
    for name, (filename, data, ctype) in files.items():
        body.write(
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
            f"Content-Type: {ctype}\r\n\r\n".encode()
        )
        body.write(data)
        body.write(b"\r\n")
    body.write(f"--{boundary}--\r\n".encode())
    req = urllib.request.Request(
        BASE + path, data=body.getvalue(),
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"}, method="POST")
    return handle(req)


def req(path: str, method="GET", body=None):
    data = json.dumps(body).encode() if body is not None else None
    req_ = urllib.request.Request(
        BASE + path, data=data, method=method,
        headers={"Content-Type": "application/json"} if data else {})
    return handle(req_)


def handle(r):
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            return resp.status, json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode("utf-8"))


def read(p, mode="rb"):
    with open(p, mode, **({"encoding": "utf-8"} if mode == "r" else {})) as f:
        return f.read()


print("== 1. 导入预览与配对 ==")
pdf_a = read(os.path.join(SAMPLES, "paper-a.pdf"))
md_a = read(os.path.join(SAMPLES, "paper-a.md"), "r")
st, prev = post_multipart("/api/import/analyze", {"pdf": ("paper-a.pdf", pdf_a, "application/pdf"), "md": ("paper-a.md", md_a.encode(), "text/markdown")})
check("analyze 返回 200", st == 200, str(prev))
check("PDF 页数=8", prev.get("pdf", {}).get("page_count") == 8, str(prev.get("pdf")))
check("固定栏目 8/8", len([s for s in prev["md"]["sections"] if s["fixed"]]) == 8)
check("无页码越界告警", len(prev.get("page_warnings", [])) == 0, str(prev.get("page_warnings")))
check("无重复", len(prev.get("duplicates", [])) == 0)

print("== 2. 正式入库 ==")
st, r = req("/api/import/commit", "POST", {
    "temp_token": prev["temp_token"], "title": prev["md"]["meta"]["title"],
    "authors": prev["md"]["meta"]["authors"], "year": prev["md"]["meta"]["year"],
    "doi": prev["md"]["meta"]["doi"], "tags": prev["md"]["meta"]["tags"],
    "project": "Topic A"})
check("commit 成功", st == 200 and r["paper"]["title"].startswith("Attention"), str(r)[:200])
id_a = r["paper"]["id"]

# paper-b：PDF + MD
st, prev_b = post_multipart("/api/import/analyze", {"pdf": ("paper-b.pdf", read(os.path.join(SAMPLES, "paper-b.pdf")), "application/pdf"), "md": ("paper-b.md", read(os.path.join(SAMPLES, "paper-b.md"), "r").encode(), "text/markdown")})
st, r = req("/api/import/commit", "POST", {"temp_token": prev_b["temp_token"], "title": prev_b["md"]["meta"]["title"], "authors": prev_b["md"]["meta"]["authors"], "year": 2023, "doi": "10.1000/sample-b", "tags": ["机器学习"], "project": "Topic A"})
id_b = r["paper"]["id"]
check("paper-b 入库", st == 200)

# paper-c：缺少 局限性 → 待补充；页码链接 12 页内
st, prev_c = post_multipart("/api/import/analyze", {"pdf": ("paper-c.pdf", read(os.path.join(SAMPLES, "paper-c.pdf")), "application/pdf"), "md": ("paper-c.md", read(os.path.join(SAMPLES, "paper-c.md"), "r").encode(), "text/markdown")})
check("paper-c 缺 局限性 被识别", "局限性" in prev_c["md"]["missing_fixed"])
st, r = req("/api/import/commit", "POST", {"temp_token": prev_c["temp_token"], "title": prev_c["md"]["meta"]["title"], "authors": prev_c["md"]["meta"]["authors"], "year": 2025, "doi": "", "tags": ["综述"], "project": ""})
id_c = r["paper"]["id"]
check("paper-c 入库且分析可对比", r["paper"]["analysis_status"] == "comparable")

print("== 3. 重复导入提示 ==")
st, dup = post_multipart("/api/import/analyze", {"pdf": ("paper-a.pdf", pdf_a, "application/pdf")})
check("相同 PDF 内容识别为重复", any(d["reason"] == "sha256" for d in dup.get("duplicates", [])), str(dup.get("duplicates")))

print("== 4. 搜索与命中来源 ==")
st, r = req("/api/papers?query=" + urllib.parse.quote("稀疏"))
check("关键词命中 paper-b", st == 200 and len(r) == 1 and r[0]["id"] == id_b, str(r)[:200])
check("命中来源=analysis", r and "analysis" in r[0].get("matched_on", []))
st, r = req("/api/papers?tag=" + urllib.parse.quote("综述"))
check("标签筛选命中 paper-c", len(r) == 1 and r[0]["id"] == id_c)

print("== 5. 笔记独立与保留 ==")
st, note = req(f"/api/papers/{id_a}/notes", "PUT", {"content": "个人判断：效率对比条件需要复核。"})
check("笔记保存", st == 200)
st, r = req("/api/papers?query=" + urllib.parse.quote("个人判断"))
check("笔记可被搜索", len(r) == 1 and r[0]["id"] == id_a)

print("== 6. 重新导入：恢复副本保留、笔记不被覆盖 ==")
md_a2 = md_a.replace("约 3.2 个百分点", "约 3.5 个百分点（修订版）")
st, prev2 = post_multipart("/api/import/analyze", {"md": ("paper-a.md", md_a2.encode(), "text/markdown")})
st, r = req("/api/import/commit", "POST", {"temp_token": prev2["temp_token"], "title": "Attention Is All You Need for Sample Domain", "authors": [], "year": 2024, "doi": "10.1000/sample-a", "tags": [], "update_paper_id": id_a})
check("更新已有文献成功", st == 200)
st, an = req(f"/api/papers/{id_a}/analysis")
check("上一次分析保留为恢复副本", an["prev_md_content"] and "3.2 个百分点" in an["prev_md_content"])
check("当前内容已更新", "3.5 个百分点" in an["md_content"])
st, note = req(f"/api/papers/{id_a}/notes")
check("笔记未被覆盖", "个人判断" in note["content"])

print("== 7. 证据核查状态 ==")
link = [p for p in an["parsed"]["page_links"] if p["page"] == 5][0]
st, ev = req(f"/api/papers/{id_a}/evidence", "PUT", {"anchor_hash": uuid.uuid4().hex, "section": "主要发现", "excerpt": link["context"], "page": 5, "check_status": "verified"})
check("核查状态保存", st == 200 and ev["check_status"] == "verified")
# 修改分析内容使该摘录消失 → 状态降级
st, _ = req(f"/api/papers/{id_a}/analysis", "PUT", {"md": an["md_content"].replace(link["context"][:20], "彻底改写的上下文XYZ")})
st, evs = req(f"/api/papers/{id_a}/evidence")
check("内容变更后核查状态降级为待核查", evs and evs[0]["check_status"] == "unverified" and evs[0]["stale"] is True, str(evs)[:200])
# 还原
st, an = req(f"/api/papers/{id_a}/analysis", "PUT", {"md": md_a2})

print("== 8. 对比：保存/更新提示/导出 ==")
st, cmp_ = req("/api/compares", "POST", {"name": "注意力对比", "paper_ids": [id_a, id_b, id_c]})
check("创建对比", st == 200 and len(cmp_["paper_ids"]) == 3)
cmp_id = cmp_["id"]
st, got = req(f"/api/compares/{cmp_id}")
check("打开对比刷新快照", st == 200)
def get_text(path: str):
    with urllib.request.urlopen(BASE + path, timeout=30) as resp:
        return resp.status, resp.read().decode("utf-8")


st, _ = req(f"/api/compares/{cmp_id}", "PUT", {"name": "注意力对比", "paper_ids": [id_a, id_b, id_c], "dimensions": ["研究问题", "研究方法", "主要发现", "局限性"], "synthesis": {"agreement": "长序列收益明确", "differences": "效率数字条件不同", "gap": "统一评测协议缺失", "conclusion": "先做协议对齐"}})
st, ex = get_text(f"/api/compares/{cmp_id}/export")
check("导出综述含综合结论", "先做协议对齐" in ex and "共识" in ex)
check("导出含页码文本", "PDF 第" in ex)
# 更新 paper-b 分析 → 重新打开对比应提示（updated_at 秒级，跨秒边界确保可见）
import time
time.sleep(1.2)
st, anb = req(f"/api/papers/{id_b}/analysis")
st, _ = req(f"/api/papers/{id_b}/analysis", "PUT", {"md": anb["md_content"] + "\n\n补充：稳定性实验通过。"})
st, got = req(f"/api/compares/{cmp_id}")
check("分析更新后打开对比有提示", id_b in got.get("updated_papers", []), str(got.get("updated_papers")))
check("综合结论未被改写", got["synthesis"]["conclusion"] == "先做协议对齐")

print("== 9. 单篇导出 ==")
st, ex = get_text(f"/api/papers/{id_a}/export?notes=true")
check("单篇导出含笔记与标题", "个人判断" in ex and "Attention" in ex)

print("== 10. PDF 服务与托管副本 ==")
with urllib.request.urlopen(f"{BASE}/api/papers/{id_a}/pdf", timeout=30) as resp:
    data = resp.read()
check("PDF 可读取（托管副本）", resp.status == 200 and data[:4] == b"%PDF")

print("== 11. 备份与恢复到新目录 ==")
backup_dir = os.path.join(ROOT, "data", "backups")
os.makedirs(backup_dir, exist_ok=True)
st, bk = req("/api/backup", "POST", {"path": backup_dir})
check("备份成功且含 3 篇", st == 200 and bk["papers"] == 3 and os.path.exists(bk["zip_path"]), str(bk))
restore_dir = os.path.join(ROOT, "data", "restored-lib")
st, rs = req("/api/restore", "POST", {"zip_path": bk["zip_path"], "target_dir": restore_dir})
check("恢复成功", st == 200 and rs["papers"] == 3, str(rs))
st, papers = req("/api/papers")
check("恢复后 3 篇全部可用", len(papers) == 3, str(len(papers)))
st, note = req(f"/api/papers/{id_a}/notes")
check("恢复后笔记保留", "个人判断" in note["content"])
st, got = req(f"/api/compares/{cmp_id}")
check("恢复后对比记录保留", st == 200 and got["name"] == "注意力对比")

print("== 12. 回收站 ==")
st, _ = req(f"/api/papers/{id_c}", "DELETE")
st, papers = req("/api/papers")
check("移入回收站后列表隐藏", len(papers) == 2)
st, papers = req("/api/papers?trash=true")
check("回收站可见", len(papers) == 1 and papers[0]["id"] == id_c)
st, _ = req(f"/api/papers/{id_c}/restore", "POST")
st, papers = req("/api/papers")
check("恢复回到列表", len(papers) == 3)

print(f"\n结果：{PASS} 通过，{FAIL} 失败")
sys.exit(1 if FAIL else 0)
