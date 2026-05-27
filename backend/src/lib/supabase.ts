import { createClient, type SupabaseClient } from "@supabase/supabase-js";

export const DEFAULT_SUPABASE_TIMEOUT_MS = 5_000;

export type SupabaseAdapterErrorCode =
  | "SUPABASE_MISCONFIGURED"
  | "SUPABASE_DEPENDENCY_FAILURE"
  | "SUPABASE_TIMEOUT";

export class SupabaseAdapterError extends Error {
  constructor(
    public code: SupabaseAdapterErrorCode,
    message: string,
    public cause?: unknown,
  ) {
    super(message);
    this.name = "SupabaseAdapterError";
  }
}

let supabase: SupabaseClient | null = null;

function getSupabaseConfig(env: NodeJS.ProcessEnv = process.env): {
  url: string;
  key: string;
} {
  const url = env.SUPABASE_URL?.trim();
  const key = env.SUPABASE_SERVICE_ROLE_KEY?.trim();

  if (!url || !key) {
    throw new SupabaseAdapterError(
      "SUPABASE_MISCONFIGURED",
      "Supabase is not configured. Set SUPABASE_URL and SUPABASE_SERVICE_ROLE_KEY to enable profile operations.",
    );
  }

  try {
    new URL(url);
  } catch (error) {
    throw new SupabaseAdapterError(
      "SUPABASE_MISCONFIGURED",
      "SUPABASE_URL must be a valid absolute URL.",
      error,
    );
  }

  return { url, key };
}

export function getSupabaseTimeoutMs(env: NodeJS.ProcessEnv = process.env): number {
  const raw = env.SUPABASE_TIMEOUT_MS;
  if (!raw) return DEFAULT_SUPABASE_TIMEOUT_MS;

  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_SUPABASE_TIMEOUT_MS;
}

export function getSupabaseClient(): SupabaseClient {
  const { url, key } = getSupabaseConfig();

  if (!supabase) {
    try {
      supabase = createClient(url, key, {
        auth: {
          autoRefreshToken: false,
          persistSession: false,
        },
      });
    } catch (error) {
      throw new SupabaseAdapterError(
        "SUPABASE_DEPENDENCY_FAILURE",
        "Failed to initialize Supabase client.",
        error,
      );
    }
  }

  return supabase;
}

export async function runSupabaseQuery<T>(
  query: PromiseLike<T>,
  operation: string,
  timeoutMs: number = getSupabaseTimeoutMs(),
): Promise<T> {
  let timer: NodeJS.Timeout | undefined;

  try {
    return await Promise.race([
      Promise.resolve(query),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          reject(
            new SupabaseAdapterError(
              "SUPABASE_TIMEOUT",
              `Supabase operation timed out: ${operation}`,
            ),
          );
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function resetSupabaseClientForTests(): void {
  if (process.env.NODE_ENV === "test") {
    supabase = null;
  }
}
