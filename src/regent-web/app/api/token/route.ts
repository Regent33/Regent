import { AccessToken } from "livekit-server-sdk";
import { type NextRequest, NextResponse } from "next/server";

// Mint a short-lived LiveKit join token server-side, so the API secret never
// reaches the browser. Works with self-hosted LiveKit OSS or LiveKit Cloud —
// only the env (URL + key/secret) changes.
export const runtime = "nodejs"; // livekit-server-sdk needs Node crypto

export async function GET(req: NextRequest) {
  const apiKey = process.env.LIVEKIT_API_KEY;
  const apiSecret = process.env.LIVEKIT_API_SECRET;
  const url = process.env.NEXT_PUBLIC_LIVEKIT_URL;
  if (!apiKey || !apiSecret) {
    return NextResponse.json({ error: "LiveKit not configured" }, { status: 503 });
  }

  // Sanitize the room name at the boundary (it comes from the client).
  const room =
    (req.nextUrl.searchParams.get("room") ?? "regent-call")
      .replace(/[^a-zA-Z0-9_-]/g, "")
      .slice(0, 64) || "regent-call";
  const identity = `caller-${Math.random().toString(36).slice(2, 8)}`;

  const at = new AccessToken(apiKey, apiSecret, { identity, ttl: "10m" });
  at.addGrant({ room, roomJoin: true, canPublish: true, canSubscribe: true });

  return NextResponse.json({ token: await at.toJwt(), url });
}
