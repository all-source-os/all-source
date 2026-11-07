package allsource

import (
	"context"
	"errors"
)

// CredentialProvider produces the bearer token for one request attempt.
//
// The client asks it on every attempt, retries included, so a short-lived
// token never outlives the request it was minted for. A returned error fails
// the request with a *CredentialError and is not retried.
type CredentialProvider interface {
	// Bearer returns the token without the "Bearer " prefix.
	Bearer(ctx context.Context) (string, error)
}

// CredentialProviderFunc adapts a function to a CredentialProvider.
type CredentialProviderFunc func(ctx context.Context) (string, error)

// Bearer calls f.
func (f CredentialProviderFunc) Bearer(ctx context.Context) (string, error) { return f(ctx) }

// WithCredentialProvider makes the client authenticate with the provider's
// bearer instead of the static API key passed to New, which may then be empty.
// Only the Authorization header is sent; X-API-Key is not.
func WithCredentialProvider(p CredentialProvider) Option {
	return func(c *Client) {
		c.cred = p
	}
}

// ErrNoCredential is wrapped by a *CredentialError when a provider returns an
// empty token.
var ErrNoCredential = errors.New("allsource: credential provider returned an empty token")

// CredentialError is returned when the CredentialProvider could not produce a
// bearer. No request was sent.
type CredentialError struct {
	Err error
}

func (e *CredentialError) Error() string { return "allsource: credential: " + e.Err.Error() }

func (e *CredentialError) Unwrap() error { return e.Err }
