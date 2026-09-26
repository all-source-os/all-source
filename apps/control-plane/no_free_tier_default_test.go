package main

import (
	"go/ast"
	"go/parser"
	"go/token"
	"strings"
	"testing"
)

// mintingSites are the EXACT (file, function) pairs that create a new tenant on a
// self-service signup. None of them may hardcode a `"free"` subscription tier as
// the creation default: every new self-service tenant must start a 14-day trial
// (prompt 048 — the marketing/catalog already say "no free plan"; the backend
// must not keep handing out permanent free).
//
// This is the structural guard that stops free from silently coming back, the
// sibling of TestNoStatusPathReferencesDemo. It scans function BODIES (not whole
// files), so the LEGITIMATE read-time fallback `const defaultPlan = "free"` and
// extractPlan's grandfathering branches in list_tenants.go are NOT flagged —
// those inspect EXISTING tenants, they do not mint new ones.
//
// If anyone reintroduces a `"free"` literal inside one of these minting paths
// (directly or by copy-pasting an old metadata block), CI fails here.
var mintingSites = []struct {
	file string
	fn   string
}{
	{file: "onboard.go", fn: "OnboardHandler"},                               // POST /api/v1/onboard/start
	{file: "auth.go", fn: "findOrCreateOAuthUser"},                           // OAuth login + email register funnel
	{file: "oauth_workspace.go", fn: "createRegisteredWorkspace"},            // durable OAuth owner bootstrap
	{file: "internal/application/usecases/register_agent.go", fn: "Execute"}, // POST /api/v1/agents/register
}

// freeNeedle is the tier literal a minting path must never set as a default.
const freeNeedle = `"free"`

// TestNoFreeTierMintedAsCreationDefault parses each minting site's function body
// and asserts it contains no `"free"` string literal — proving no self-service
// tenant-creation path defaults to the (now-retired-for-new-signups) free tier.
func TestNoFreeTierMintedAsCreationDefault(t *testing.T) {
	fset := token.NewFileSet()

	for _, site := range mintingSites {
		file, err := parser.ParseFile(fset, site.file, nil, 0)
		if err != nil {
			t.Fatalf("parse %s: %v", site.file, err)
		}

		var scanned bool
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil || fn.Name.Name != site.fn {
				continue
			}
			scanned = true

			ast.Inspect(fn.Body, func(n ast.Node) bool {
				lit, ok := n.(*ast.BasicLit)
				if !ok || lit.Kind != token.STRING {
					return true
				}
				if strings.Contains(lit.Value, freeNeedle) {
					pos := fset.Position(lit.Pos())
					t.Errorf("minting path %s in %s sets %q at line %d — a new self-service "+
						"tenant must start a 14-day trial, NOT free. Use "+
						"usecases.TrialSubscriptionMetadata / TrialQuotaMetadata instead "+
						"(prompt 048). The read-time fallback `defaultPlan` is the only "+
						"place \"free\" is allowed, and it is not a creation default.",
						site.fn, site.file, lit.Value, pos.Line)
				}
				return true
			})
		}

		// Guard against silent drift: if a minting function is renamed/removed,
		// this test would otherwise pass vacuously.
		if !scanned {
			t.Errorf("expected to scan minting function %q in %s but it was not found — "+
				"if it was renamed/moved, update mintingSites so the no-free guard keeps covering it",
				site.fn, site.file)
		}
	}
}
