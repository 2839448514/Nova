import type {
  AskUserAnswerSubmission,
  NeedsUserInputPayload,
  PlanModeChangePayload,
} from "./chat-types";

export function buildPendingQuestionReply(
  payload: AskUserAnswerSubmission | null,
  source: "submit" | "skip",
): string {
  if (source === "skip" || !payload) {
    return "用户跳过了澄清问题，请基于当前上下文继续；如果仍有缺失，请明确说明你做了哪些假设。";
  }

  const lines: string[] = ["用户补充了以下澄清信息："];
  for (const [question, answer] of Object.entries(payload.answers)) {
    const answerText = Array.isArray(answer) ? answer.join("、") : answer;
    if (answerText.trim()) {
      lines.push(`- ${question}：${answerText}`);
    }
  }

  if (payload.freeform?.trim()) {
    lines.push(`- 其他补充：${payload.freeform.trim()}`);
  }

  return lines.join("\n");
}

export function renderToolResult(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";

  try {
    const parsed = JSON.parse(trimmed) as NeedsUserInputPayload & {
      content?: Array<{ type?: string; text?: string }>;
    };
    if (parsed?.type === "needs_user_input") {
      const lines: string[] = [];
      if (parsed.context) {
        lines.push(`需要你补充信息：${parsed.context}`);
      }
      if (Array.isArray(parsed.questions) && parsed.questions.length > 0) {
        lines.push("");
        for (const item of parsed.questions) {
          lines.push(`${item.header}: ${item.question}`);
          for (const opt of item.options ?? []) {
            lines.push(`- ${opt.label}`);
          }
          lines.push("");
        }
      }
      if (parsed.allow_freeform) {
        lines.push("也可以直接描述你的具体需求。");
      }
      return lines.join("\n");
    }

    if (Array.isArray(parsed?.content)) {
      const texts = parsed.content
        .filter((b) => b && (b.type === "text" || typeof b.text === "string"))
        .map((b) => (b.text ?? "").trim())
        .filter(Boolean);
      if (texts.length > 0) {
        return texts.join("\n\n");
      }
    }

    return JSON.stringify(parsed, null, 2);
  } catch {
    return trimmed;
  }
}

export function parseNeedsUserInput(raw: string): NeedsUserInputPayload | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  try {
    const parsed = JSON.parse(trimmed) as NeedsUserInputPayload;
    if (
      parsed?.type === "needs_user_input" &&
      Array.isArray(parsed.questions) &&
      parsed.questions.length > 0
    ) {
      return {
        type: parsed.type,
        context: parsed.context,
        allow_freeform: parsed.allow_freeform ?? true,
        questions: parsed.questions,
      };
    }
  } catch {
    return null;
  }
  return null;
}

export function parsePlanModeChange(raw: string): PlanModeChangePayload | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  try {
    const parsed = JSON.parse(trimmed) as PlanModeChangePayload;
    if (parsed?.type === "plan_mode_change" && parsed.mode) {
      return parsed;
    }
  } catch {
    return null;
  }
  return null;
}
