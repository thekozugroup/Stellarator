// Dexie-backed local persistence for chat threads + messages.
"use client";

import Dexie, { type Table } from "dexie";
import type { ChatMessage, ChatThread } from "./types";

class ChatDB extends Dexie {
  threads!: Table<ChatThread, string>;
  messages!: Table<ChatMessage, string>;

  constructor() {
    super("stellarator.chat");
    this.version(1).stores({
      threads: "id, updatedAt, archived",
      messages: "id, threadId, createdAt, [threadId+createdAt]",
    });
  }
}

let _db: ChatDB | null = null;
export function db(): ChatDB {
  if (!_db) _db = new ChatDB();
  return _db;
}
