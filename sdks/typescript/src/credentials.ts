/**
 * Produces the bearer token for one request attempt, without the `Bearer `
 * prefix.
 *
 * Either a function or an object with a `bearer()` method, such as
 * `Es256AssertionProvider`. The client asks it on every attempt, retries
 * included, so a short-lived token never outlives the request it was minted
 * for. Throw when no token can be produced; the client does not retry that.
 */
export type CredentialProvider =
  | (() => string | Promise<string>)
  | { bearer(): string | Promise<string> };

/**
 * A credential could not be produced: the provider threw or returned an empty
 * bearer, or an assertion provider was given an unusable key or identity.
 * Never retried and never counted by the circuit breaker.
 */
export class CredentialError extends Error {
  constructor(
    message: string,
    public readonly cause?: unknown
  ) {
    super(message);
    this.name = "CredentialError";
  }
}

export async function resolveBearer(provider: CredentialProvider): Promise<string> {
  let bearer: unknown;
  try {
    bearer = typeof provider === "function" ? await provider() : await provider.bearer();
  } catch (error) {
    if (error instanceof CredentialError) throw error;
    const detail = error instanceof Error ? error.message : String(error);
    throw new CredentialError(`credential provider failed: ${detail}`, error);
  }
  if (typeof bearer !== "string" || bearer === "") {
    throw new CredentialError("credential provider returned an empty bearer");
  }
  return bearer;
}
