const mockCreateClient = jest.fn();

jest.mock("@supabase/supabase-js", () => ({
  createClient: (...args: any[]) => mockCreateClient(...args),
}));

describe("Supabase adapter", () => {
  const originalEnv = process.env;

  beforeEach(() => {
    jest.resetModules();
    jest.clearAllMocks();
    process.env = {
      ...originalEnv,
      NODE_ENV: "test",
      SUPABASE_URL: "https://example.supabase.co",
      SUPABASE_SERVICE_ROLE_KEY: "service-role-key",
    };
  });

  afterEach(() => {
    jest.useRealTimers();
    process.env = originalEnv;
  });

  it("initializes the client from environment config and caches it", async () => {
    const client = { from: jest.fn() };
    mockCreateClient.mockReturnValue(client);
    const { getSupabaseClient, resetSupabaseClientForTests } = await import("../lib/supabase");

    resetSupabaseClientForTests();

    expect(getSupabaseClient()).toBe(client);
    expect(getSupabaseClient()).toBe(client);
    expect(mockCreateClient).toHaveBeenCalledTimes(1);
    expect(mockCreateClient).toHaveBeenCalledWith(
      "https://example.supabase.co",
      "service-role-key",
      {
        auth: {
          autoRefreshToken: false,
          persistSession: false,
        },
      },
    );
  });

  it("throws a stable adapter error when required env config is missing", async () => {
    delete process.env.SUPABASE_URL;
    const { getSupabaseClient, SupabaseAdapterError } = await import("../lib/supabase");

    try {
      getSupabaseClient();
      throw new Error("expected getSupabaseClient to throw");
    } catch (error) {
      expect(error).toBeInstanceOf(SupabaseAdapterError);
      expect(error).toMatchObject({ code: "SUPABASE_MISCONFIGURED" });
    }
    expect(mockCreateClient).not.toHaveBeenCalled();
  });

  it("throws a stable adapter error when Supabase URL is invalid", async () => {
    process.env.SUPABASE_URL = "not-a-url";
    const { getSupabaseClient } = await import("../lib/supabase");

    try {
      getSupabaseClient();
      throw new Error("expected getSupabaseClient to throw");
    } catch (error) {
      expect(error).toMatchObject({ code: "SUPABASE_MISCONFIGURED" });
    }
  });

  it("translates createClient dependency failures", async () => {
    mockCreateClient.mockImplementation(() => {
      throw new Error("sdk unavailable");
    });
    const { getSupabaseClient } = await import("../lib/supabase");

    try {
      getSupabaseClient();
      throw new Error("expected getSupabaseClient to throw");
    } catch (error) {
      expect(error).toMatchObject({ code: "SUPABASE_DEPENDENCY_FAILURE" });
    }
  });

  it("falls back to the default timeout when env timeout is invalid", async () => {
    process.env.SUPABASE_TIMEOUT_MS = "-1";
    const { DEFAULT_SUPABASE_TIMEOUT_MS, getSupabaseTimeoutMs } = await import("../lib/supabase");

    expect(getSupabaseTimeoutMs()).toBe(DEFAULT_SUPABASE_TIMEOUT_MS);
  });

  it("rejects long-running operations with a stable timeout error", async () => {
    jest.useFakeTimers();
    const { runSupabaseQuery } = await import("../lib/supabase");
    const pending = new Promise(() => undefined);

    const result = runSupabaseQuery(pending, "test operation", 25);
    jest.advanceTimersByTime(25);

    await expect(result).rejects.toMatchObject({
      code: "SUPABASE_TIMEOUT",
      message: "Supabase operation timed out: test operation",
    });
  });
});
