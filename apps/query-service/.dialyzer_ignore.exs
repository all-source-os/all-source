[
  # JOSE library returns {true/false, JWT, JWS}, not {:error, reason}
  # These defensive pattern matches are unreachable per dialyzer
  ~r/user_socket\.ex.*pattern_match/,
  ~r/auth_pipeline\.ex.*pattern_match/,
  ~r/jwt_auth\.ex.*pattern_match/,
  # Pattern match coverage warnings (exhaustive matches flagged by dialyzer)
  ~r/query_controller\.ex.*pattern_match_cov/,
  ~r/tenant_context\.ex.*pattern_match_cov/,
  # Mint's connection types report either warning class across OTP releases.
  ~r/core_websocket_worker\.ex.*(?:pattern_match|call_with_opaque)/
]
