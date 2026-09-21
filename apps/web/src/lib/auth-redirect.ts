/** Only allow application paths, never external or API destinations. */
export function authRedirect(next: string | null, isNewUser: boolean): string {
  if (
    next?.startsWith("/") &&
    !next.startsWith("//") &&
    !next.includes("\\") &&
    !Array.from(next).some((character) => character.charCodeAt(0) <= 32) &&
    !/^\/api(?:[/?#]|$)/.test(next)
  )
    return next;
  return isNewUser ? "/onboarding" : "/dashboard";
}
