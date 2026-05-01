"use client";

import { use } from "react";
import { ChatPanel } from "@/components/chat/chat-panel";

export default function ChatThreadPage({
  params,
}: {
  params: Promise<{ threadId: string }>;
}) {
  const { threadId } = use(params);
  return <ChatPanel threadId={threadId} />;
}
