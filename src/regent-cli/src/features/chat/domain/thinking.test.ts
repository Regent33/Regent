import { expect, test } from "bun:test";
import { spaceEmoji, splitThinking } from "@features/chat/domain/thinking.ts";

test("splits a complete think block from the answer", () => {
  expect(splitThinking("<think>reasoning</think>the answer")).toEqual({
    thinking: "reasoning",
    answer: "the answer",
  });
});

test("treats an unclosed trailing <think> as in-progress thinking (streaming)", () => {
  expect(splitThinking("<think>still thinking")).toEqual({
    thinking: "still thinking",
    answer: "",
  });
});

test("strips a stray </think> with no opening — no raw tag leaks", () => {
  expect(splitThinking("</think>")).toEqual({ thinking: "", answer: "" });
  expect(splitThinking("answer </think> more")).toEqual({ thinking: "", answer: "answer  more" });
});

test("plain text passes through untouched", () => {
  expect(splitThinking("just a normal reply")).toEqual({
    thinking: "",
    answer: "just a normal reply",
  });
});

test("spaceEmoji separates emojis glued to words, leaves spaced ones alone", () => {
  expect(spaceEmoji("🎉Great")).toBe("🎉 Great");
  expect(spaceEmoji("done✅now")).toBe("done ✅ now");
  expect(spaceEmoji("✻ Thinking")).toBe("✻ Thinking");
  expect(spaceEmoji("no emoji here")).toBe("no emoji here");
});
