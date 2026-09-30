---
schema_version: 1
title: "Efficient Training via Sparse Attention"
authors: ["Carol Wu"]
year: 2023
doi: "10.1000/sample-b"
source_pdf: "paper-b.pdf"
tags:
  - "机器学习"
---

# Efficient Training via Sparse Attention

## 一句话总结

稀疏注意力在保持精度的同时把长序列训练成本降低了一个数量级。

## 研究问题

全注意力二次方复杂度如何在保持表达能力的同时降低？

## 研究方法

分块稀疏注意力 + 局部窗口；在四个公开数据集上与全注意力对照。

## 样本与数据

四个数据集，序列长度 4K～32K；详见 [PDF 第 4 页](#pdf-page-4)。

## 主要发现

- 32K 长度下显存占用减少 62% [PDF 第 7 页](#pdf-page-7)。
- 精度差异在 0.5 个百分点以内，属统计不显著。

## 创新点

分块策略可根据数据自适应，无需人工设定窗口。

## 局限性

对极短序列（<1K）没有优势；实现依赖特定硬件指令。

## 阅读重点

1. 第 7 页的显存对比是否包含激活值。
2. 自适应分块的开销是否被计入。
