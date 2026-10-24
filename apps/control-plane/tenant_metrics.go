package main

import (
	"io"
	"log"
	"net/http"
	"net/url"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

// tenantMetricsRegistry holds every series with a tenant label. It is served
// only by the private listener; the public /metrics route never gathers it.
var tenantMetricsRegistry = prometheus.NewRegistry()

var tenantMetrics = promauto.With(tenantMetricsRegistry)

const (
	defaultMetricsAddr = ":9091"
	anonymousTenant    = "anonymous"
	unmatchedRoute     = "unmatched"
	overflowLabel      = "other"
	maxTenantLabels    = 256
	maxNamespaceLabels = 32
)

var (
	tenantResponseBytes = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
		Name: "control_plane_tenant_response_bytes_total",
		Help: "Response body bytes written to clients, by authenticated tenant and route; excludes headers and TLS.",
	}, []string{"tenant", "route", "method", "status_class"})
	tenantRequestBytes = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
		Name: "control_plane_tenant_request_bytes_total",
		Help: "Request body bytes read from clients, by authenticated tenant and route.",
	}, []string{"tenant", "route", "method"})
	tenantRequests = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
		Name: "control_plane_tenant_requests_total",
		Help: "Requests served, by authenticated tenant and route.",
	}, []string{"tenant", "route", "method", "status_class"})
)

// boundedLabels caps a label's distinct values; later values share overflowLabel.
type boundedLabels struct {
	mu    sync.Mutex
	seen  map[string]struct{}
	limit int
}

func newBoundedLabels(limit int) *boundedLabels {
	return &boundedLabels{seen: map[string]struct{}{}, limit: limit}
}

func (b *boundedLabels) label(value string) string {
	b.mu.Lock()
	defer b.mu.Unlock()
	if _, ok := b.seen[value]; ok {
		return value
	}
	if len(b.seen) >= b.limit {
		return overflowLabel
	}
	b.seen[value] = struct{}{}
	return value
}

var (
	tenantLabels    = newBoundedLabels(maxTenantLabels)
	namespaceLabels = newBoundedLabels(maxNamespaceLabels)
)

func tenantLabel(tenant string) string {
	if tenant == "" {
		return anonymousTenant
	}
	return tenantLabels.label(tenant)
}

func queryNamespaceLabel(q url.Values) string {
	namespace := "none"
	if eventType := q.Get("event_type"); eventType != "" {
		namespace = queryNamespace(eventType, ".")
	} else if prefix := q.Get("event_type_prefix"); prefix != "" {
		namespace = queryNamespace(prefix, ".")
	} else if entity := q.Get("entity_id"); entity != "" {
		namespace = queryNamespace(entity, ":")
	}
	return namespaceLabels.label(namespace)
}

func methodLabel(method string) string {
	switch method {
	case http.MethodGet, http.MethodHead, http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete, http.MethodOptions:
		return method
	}
	return overflowLabel
}

func statusClass(status int) string {
	if status < 100 || status > 599 {
		return overflowLabel
	}
	return strconv.Itoa(status/100) + "xx"
}

type meteredWriter struct {
	gin.ResponseWriter
	written atomic.Int64
}

func (w *meteredWriter) Write(data []byte) (int, error) {
	n, err := w.ResponseWriter.Write(data)
	w.written.Add(int64(n))
	return n, err
}

func (w *meteredWriter) WriteString(value string) (int, error) {
	n, err := w.ResponseWriter.WriteString(value)
	w.written.Add(int64(n))
	return n, err
}

type meteredBody struct {
	io.ReadCloser
	read atomic.Int64
}

func (b *meteredBody) Read(p []byte) (int, error) {
	n, err := b.ReadCloser.Read(p)
	b.read.Add(int64(n))
	return n, err
}

// TenantMetering counts the bytes each tenant sends and receives. It must be
// registered before every middleware that writes a response so it wraps the
// final writer; the tenant is read after the chain, once AuthMiddleware ran.
func TenantMetering() gin.HandlerFunc {
	return func(c *gin.Context) {
		writer := &meteredWriter{ResponseWriter: c.Writer}
		c.Writer = writer
		var body *meteredBody
		if c.Request.Body != nil && c.Request.Body != http.NoBody {
			body = &meteredBody{ReadCloser: c.Request.Body}
			c.Request.Body = body
		}
		defer func() {
			tenant, _ := authTenantFromContext(c)
			label := tenantLabel(tenant)
			route := c.FullPath()
			if route == "" {
				route = unmatchedRoute
			}
			method := methodLabel(c.Request.Method)
			class := statusClass(writer.Status())
			tenantRequests.WithLabelValues(label, route, method, class).Inc()
			tenantResponseBytes.WithLabelValues(label, route, method, class).Add(float64(writer.written.Load()))
			read := int64(0)
			if body != nil {
				read = body.read.Load()
			}
			tenantRequestBytes.WithLabelValues(label, route, method).Add(float64(read))
		}()
		c.Next()
	}
}

func publicMetricsHandler() http.Handler { return promhttp.Handler() }

func privateMetricsHandler() http.Handler {
	return promhttp.HandlerFor(prometheus.Gatherers{prometheus.DefaultGatherer, tenantMetricsRegistry}, promhttp.HandlerOpts{})
}

// newPrivateMetricsServer serves per-tenant metrics on METRICS_ADDR. Fly
// exposes only [http_service] internal_port publicly, but every machine on the
// org's 6PN network can reach this port. GET only: Core webhook delivery POSTs
// to tenant-chosen URLs and stores the response body where the tenant reads it.
func newPrivateMetricsServer(lookup func(string) string) *http.Server {
	addr := lookup("METRICS_ADDR")
	if addr == "" {
		addr = defaultMetricsAddr
	}
	mux := http.NewServeMux()
	mux.Handle("GET /metrics", privateMetricsHandler())
	return &http.Server{Addr: addr, Handler: mux, ReadHeaderTimeout: 10 * time.Second}
}

func startPrivateMetricsServer(srv *http.Server) {
	go func() {
		log.Printf("Private metrics listening on %s", srv.Addr) //nolint:gosec // G706: operator config, not user input
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Printf("Private metrics server failed: %v", err)
		}
	}()
}
