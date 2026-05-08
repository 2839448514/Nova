export type UserFacingError = {
  title: string;
  message: string;
};

export type BackendErrorPayload = {
  source?: string;
  stage?: string | null;
  code?: string;
  title?: string;
  message?: string;
};

function normalizeText(value: unknown): string {
  if (value instanceof Error) {
    return value.message;
  }
  if (typeof value === "string") {
    return value;
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function classifyRawText(rawText: string): UserFacingError {
  const raw = rawText.trim();
  const normalized = raw.toLowerCase();

  if (normalized.includes("support image input") || normalized.includes("image input")) {
    return {
      title: "当前模型不支持图片输入",
      message: "请切换到支持图片输入的模型，或移除图片后再发送。",
    };
  }

  if (normalized.includes("401") || normalized.includes("unauthorized") || normalized.includes("api key")) {
    return {
      title: "模型服务认证失败",
      message: "请检查 API Key、Provider 配置或账号权限后再试。",
    };
  }

  if (normalized.includes("403") || normalized.includes("forbidden")) {
    return {
      title: "模型服务拒绝了本次请求",
      message: "当前账号或模型权限不足，请检查服务端授权配置。",
    };
  }

  if (normalized.includes("429") || normalized.includes("rate limit") || normalized.includes("too many requests")) {
    return {
      title: "模型服务当前较忙",
      message: "请求频率过高或服务限流，请稍后再试。",
    };
  }

  if (normalized.includes("timed out") || normalized.includes("timeout")) {
    return {
      title: "请求模型服务超时",
      message: "服务响应时间过长，请稍后重试。",
    };
  }

  if (
    normalized.includes("dns")
    || normalized.includes("connection refused")
    || normalized.includes("connection reset")
    || normalized.includes("network")
    || normalized.includes("failed to send request")
  ) {
    return {
      title: "无法连接到模型服务",
      message: "请检查网络连接、服务地址或代理配置后再试。",
    };
  }

  if (
    normalized.includes("permission denied")
    || normalized.includes("access is denied")
    || normalized.includes("权限")
  ) {
    return {
      title: "当前操作缺少权限",
      message: "请检查文件权限、目录权限或当前运行环境的授权设置。",
    };
  }

  if (
    normalized.includes("no such file")
    || normalized.includes("not found")
    || normalized.includes("系统找不到")
    || normalized.includes("文件不存在")
  ) {
    return {
      title: "需要的资源不存在",
      message: "请确认文件、会话资源或服务端点仍然可用。",
    };
  }

  return {
    title: "请求处理失败",
    message: "当前请求未能完成，请稍后重试。",
  };
}

export function getUserFacingError(err: unknown): UserFacingError {
  const rawText = normalizeText(err);
  if (!rawText.trim()) {
    return {
      title: "请求处理失败",
      message: "当前请求未能完成，请稍后重试。",
    };
  }
  return classifyRawText(rawText);
}

export function formatUserFacingError(err: unknown): string {
  const friendly = getUserFacingError(err);
  return `${friendly.title}：${friendly.message}`;
}

export function formatBackendErrorEvent(payload: BackendErrorPayload): string {
  const title = (payload.title ?? "").trim();
  const message = (payload.message ?? "").trim();

  if (title && message) {
    return `${title}：${message}`;
  }
  if (message) {
    return message;
  }
  if (title) {
    return title;
  }
  return "后端处理失败，请稍后重试。";
}
