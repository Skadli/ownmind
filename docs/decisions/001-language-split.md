# 001 语言划分：agent 用 TypeScript，其余用 Rust

- 状态：已采纳（用户指定）
- 日期：2026-10-15

## 问题
agent 内核要用 pi（TypeScript 生态），其余服务端需要选语言。

## 决定
agent 模块使用 TypeScript + pi（npm 包已从 @mariozechner/* 迁到 @earendil-works/*）；其余部分全部使用 Rust。

## 考虑过的方案（及放弃原因）
- 全 TypeScript：用户指定 Rust 做非 agent 部分。
- 用 pi 的 Python 移植版：非官方，跟进慢。

## 后果
两种运行时并存，Rust 与 agent 模块之间的通信方式需在 phase-01 定（进程间协议、谁管理谁的生命周期）。
