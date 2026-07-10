// Clipboard writes that survive the Tauri webview: on Windows the app origin
// (http://tauri.localhost) is not a secure context, so `navigator.clipboard`
// can be undefined and the async API throws before copying. Try it first,
// then fall back to the classic hidden-textarea + execCommand path.
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard !== undefined) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // fall through to the legacy path
  }
  const area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.opacity = '0';
  document.body.appendChild(area);
  area.select();
  let ok = false;
  try {
    ok = document.execCommand('copy');
  } catch {
    ok = false;
  }
  area.remove();
  return ok;
}
