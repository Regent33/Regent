// Epoch seconds → "YYYY-MM-DD HH:MM" in the user's own timezone.
//
// This existed three times as `new Date(e * 1000).toISOString().slice(0, 16)`,
// which is UTC. On this machine (UTC+8) a session started at 03:49 listed as
// "2026-07-28 19:49" — the day before, eight hours off, with nothing marking it
// as UTC. The deacon's log lines are local, so the two surfaces disagreed about
// when the same event happened.
export function fmtTime(epoch: number): string {
  const d = new Date(epoch * 1000);
  if (Number.isNaN(d.getTime())) return "-";
  const p = (n: number): string => String(n).padStart(2, "0");
  // Built from local getters rather than toLocaleString: the column is
  // width-sensitive, and locale formats vary in both length and field order.
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
