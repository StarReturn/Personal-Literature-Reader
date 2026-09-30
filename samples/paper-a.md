---
schema_version: 1
title: "Attention Is All You Need for Sample Domain"
authors:
  - "Alice Zhang"
  - "Bob Li"
year: 2024
doi: "10.1000/sample-a"
source_pdf: "paper-a.pdf"
tags:
  - "机器学习"
  - "对比研究"
---

# Attention Is All You Need for Sample Domain

## 一句话总结

本文提出一种在示例领域使用注意力机制替代循环结构的方法，在标准测试集上取得一致提升。

## 研究问题

循环结构在长序列上的串行依赖是否可以被完全替代？

- 子问题 1：注意力机制能否捕获长程依赖。
- 子问题 2：训练效率能否显著提升。

## 研究方法

对照实验：同一数据集上比较循环基线与注意力方法。

> "We replace the recurrence with multi-head attention entirely." [PDF 第 3 页](#pdf-page-3)

## 样本与数据

| 数据集 | 样本量 | 划分 |
| --- | --- | --- |
| Sample-Corpus | 10000 | 8:1:1 |

评价指标为准确率与训练时长。

## 主要发现

- 长序列任务上准确率提升约 3.2 个百分点 [PDF 第 5 页](#pdf-page-5)。
- 训练时间减少约 40%（见第 6 页表 2）[PDF 第 6 页](#pdf-page-6)。

## 创新点

首次在示例领域完整移除循环结构。

## 局限性

样本领域单一；未在小数据场景验证。

## 阅读重点

1. 第 5 页图 2 的提升是否稳定。
2. 第 6 页表 2 的效率对比条件是否公平。
