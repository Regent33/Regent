// Split model output into reasoning (`<think>…</think>`) and the visible answer.
// Some models (e.g. minimax via OpenRouter) emit chain-of-thought inline as
// <think> blocks; rather than leak raw tags we render that dim/italic like
// Claude Code's "✻ Thinking". Handles complete blocks, an unclosed trailing
// <think> (mid-stream), and stray tags.
// Models often glue emojis to words ("🎉Great", "done✅") which reads cramped in
// the terminal — ensure a single space between an emoji and adjacent text.
export function spaceEmoji(text: string): string {
  return text
    .replace(/(\p{Extended_Pictographic})(?=[\p{L}\p{N}])/gu, "$1 ")
    .replace(/([\p{L}\p{N}])(?=\p{Extended_Pictographic})/gu, "$1 ");
}

export function splitThinking(text: string): { thinking: string; answer: string } {
  const answerParts: string[] = [];
  let thinking = "";
  const re = /<think>([\s\S]*?)<\/think>/gi;
  let last = 0;
  let m: RegExpExecArray | null = re.exec(text);
  while (m !== null) {
    answerParts.push(text.slice(last, m.index));
    thinking += `${m[1] ?? ""}\n`;
    last = re.lastIndex;
    m = re.exec(text);
  }
  let tail = text.slice(last);
  // An unclosed trailing <think> (streaming): treat the remainder as thinking.
  const open = tail.toLowerCase().lastIndexOf("<think>");
  if (open !== -1 && !tail.toLowerCase().includes("</think>", open)) {
    thinking += tail.slice(open + "<think>".length);
    tail = tail.slice(0, open);
  }
  answerParts.push(tail);
  const answer = answerParts
    .join("")
    .replace(/<\/?think>/gi, "")
    .trim();
  return { thinking: thinking.trim(), answer };
}
