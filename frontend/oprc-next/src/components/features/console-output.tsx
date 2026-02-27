"use client";

import { useRef, useEffect } from "react";

export type ConsoleMessageType = "info" | "error" | "success" | "warn";

export interface ConsoleMessage {
  id: string;
  type: ConsoleMessageType;
  text: string;
  timestamp: Date;
}

export interface ConsoleOutputProps {
  messages: ConsoleMessage[];
  className?: string;
}

const TYPE_COLORS: Record<ConsoleMessageType, string> = {
  info: "text-blue-400",
  error: "text-red-400",
  success: "text-green-400",
  warn: "text-yellow-400",
};

const TYPE_PREFIX: Record<ConsoleMessageType, string> = {
  info: "[INFO]",
  error: "[ERROR]",
  success: "[OK]",
  warn: "[WARN]",
};

/**
 * Terminal-style console output panel for compile/deploy feedback.
 */
export function ConsoleOutput({ messages, className }: ConsoleOutputProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  return (
    <div
      ref={scrollRef}
      className={`bg-[#1e1e1e] text-gray-300 font-mono text-xs overflow-y-auto p-3 rounded-md ${className ?? ""}`}
      data-testid="console-output"
      role="log"
      aria-label="Console output"
    >
      {messages.length === 0 ? (
        <span className="text-gray-500 italic">
          Console output will appear here...
        </span>
      ) : (
        messages.map((msg) => (
          <div key={msg.id} className="leading-5">
            <span className="text-gray-500">
              {msg.timestamp.toLocaleTimeString()}
            </span>{" "}
            <span className={TYPE_COLORS[msg.type]}>
              {TYPE_PREFIX[msg.type]}
            </span>{" "}
            <span className={msg.type === "error" ? "text-red-300" : ""}>
              {msg.text}
            </span>
          </div>
        ))
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helper to create console messages
// ---------------------------------------------------------------------------

let _nextId = 0;

export function createConsoleMessage(
  type: ConsoleMessageType,
  text: string,
): ConsoleMessage {
  return {
    id: `msg-${++_nextId}-${Date.now()}`,
    type,
    text,
    timestamp: new Date(),
  };
}
