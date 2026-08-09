/**
 * Compact a routed model id for display only. The internal canonical id keeps
 * both segments (`nvidia/nvidia/model`) so provider routing can distinguish the
 * configured provider from the vendor-prefixed model id.
 */
export function displayModel(model: string): string {
  const [provider, vendor, ...rest] = model.split("/");
  if (provider && vendor === provider && rest.length > 0) {
    return [provider, ...rest].join("/");
  }
  return model;
}
