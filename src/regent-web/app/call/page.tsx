import { CallStage } from "@/components/CallStage";

// `/call` mirrors the root so the URL printed by other surfaces (and the plain
// Python server's /call) lands on the same Jarvis call UI.
export default function CallPage() {
  return <CallStage />;
}
