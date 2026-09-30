# -*- coding: utf-8 -*-
"""生成用于验收测试的多页简单 PDF（无第三方依赖，手写 PDF 语法）。"""
import sys


def make_pdf(path: str, title: str, pages: int) -> None:
    objects = []  # (obj_id, body_bytes)

    def add(body: bytes) -> int:
        objects.append(body)
        return len(objects)

    font_id = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")

    page_ids = []
    for i in range(1, pages + 1):
        text = f"{title} - Page {i} of {pages}".encode("latin-1")
        stream = b"BT /F1 20 Tf 72 720 Td (" + text + b") Tj ET\n"
        stream += b"BT /F1 12 Tf 72 690 Td (Sample literature for acceptance testing.) Tj ET"
        content_id = add(b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream")
        page_ids.append(
            add(
                b"<< /Type /Page /Parent PARENT /MediaBox [0 0 612 792] /Contents "
                + str(content_id).encode()
                + b" 0 R /Resources << /Font << /F1 "
                + str(font_id).encode()
                + b" 0 R >> >> >>"
            )
        )

    kids = b" ".join(f"{pid} 0 R".encode() for pid in page_ids)
    pages_id = add(b"<< /Type /Pages /Kids [" + kids + b"] /Count " + str(pages).encode() + b" >>")
    catalog_id = add(b"<< /Type /Catalog /Pages " + str(pages_id).encode() + b" 0 R >>")

    # 替换 Parent 占位
    fixed = []
    for body in objects:
        if isinstance(body, bytes) and b"PARENT" in body:
            body = body.replace(b"PARENT", str(pages_id).encode() + b" 0 R")
        fixed.append(body)
    objects = fixed

    out = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
    xref_pos = len(out)
    n = len(objects) + 1
    out += f"xref\n0 {n}\n".encode()
    out += b"0000000000 65535 f \n"
    for off in offsets[1:]:
        out += f"{off:010d} 00000 n \n".encode()
    out += (
        f"trailer\n<< /Size {n} /Root {catalog_id} 0 R >>\nstartxref\n{xref_pos}\n%%EOF".encode()
    )
    with open(path, "wb") as f:
        f.write(bytes(out))


if __name__ == "__main__":
    out_path = sys.argv[1]
    title = sys.argv[2] if len(sys.argv) > 2 else "Sample Paper"
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 8
    make_pdf(out_path, title, n)
    print(f"written {out_path} ({n} pages)")
