// Thread lifecycle helpers used by layout + page redirects.
"use client";

import { nanoid } from "nanoid";
import { db } from "./db";
import type { ChatThread, ModelId } from "./types";
import { driverFor } from "./types";

export async function createThread(model: ModelId = "claude-code"): Promise<ChatThread> {
  const now = Date.now();
  const t: ChatThread = {
    id: nanoid(12),
    title: "New thread",
    model,
    driver: driverFor(model),
    createdAt: now,
    updatedAt: now,
    archived: 0,
  };
  await db().threads.put(t);
  return t;
}

export async function deleteThread(id: string): Promise<void> {
  await db().transaction("rw", db().threads, db().messages, async () => {
    await db().messages.where("threadId").equals(id).delete();
    await db().threads.delete(id);
  });
}

export async function mostRecentThreadId(): Promise<string | null> {
  const t = await db().threads.orderBy("updatedAt").reverse().first();
  return t?.id ?? null;
}

export async function ensureThread(model: ModelId = "claude-code"): Promise<string> {
  const id = await mostRecentThreadId();
  if (id) return id;
  const t = await createThread(model);
  return t.id;
}
