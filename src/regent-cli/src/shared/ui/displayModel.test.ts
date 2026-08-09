import { expect, test } from "bun:test";
import { displayModel } from "./displayModel";

test("compacts only a duplicated provider prefix for display", () => {
  expect(displayModel("nvidia/nvidia/nemotron")).toBe("nvidia/nemotron");
  expect(displayModel("openrouter/openai/gpt-5")).toBe("openrouter/openai/gpt-5");
  expect(displayModel("groq/llama-3.3-70b")).toBe("groq/llama-3.3-70b");
});
