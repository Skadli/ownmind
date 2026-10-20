/** 工具参数摘要：优先命令、路径、查询或网址，压成一行并截短。 */
export function argSummary(args: Record<string, unknown> | undefined): string {
  const arg = args?.command ?? args?.path ?? args?.query ?? args?.url ?? "";
  return String(arg).replace(/\s+/g, " ").slice(0, 80);
}
