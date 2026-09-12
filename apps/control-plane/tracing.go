package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"time"

	"github.com/gin-gonic/gin"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.4.0"
	"go.opentelemetry.io/otel/trace"
)

const (
	serviceName    = "allsource-control-plane"
	serviceVersion = "0.24.0"
)

// TracingConfig holds OpenTelemetry configuration
type TracingConfig struct {
	Enabled      bool
	OTLPEndpoint string // OTLP HTTP endpoint (e.g., "localhost:4318")
	SamplingRate float64
}

// InitTracing initializes OpenTelemetry with OTLP HTTP exporter
func InitTracing(config TracingConfig) (func(context.Context) error, error) {
	if !config.Enabled {
		log.Println("📊 Tracing disabled")
		return func(_ context.Context) error { return nil }, nil
	}

	ctx := context.Background()

	// Create OTLP HTTP exporter (works with Jaeger, Tempo, and other OTLP-compatible backends)
	opts := []otlptracehttp.Option{
		otlptracehttp.WithEndpoint(config.OTLPEndpoint),
		otlptracehttp.WithInsecure(), // Use WithInsecure for local/dev; remove for production with TLS
	}
	exp, err := otlptracehttp.New(ctx, opts...)
	if err != nil {
		return nil, fmt.Errorf("failed to create OTLP exporter: %w", err)
	}

	// Create resource with service information
	res, err := resource.Merge(
		resource.Default(),
		resource.NewWithAttributes(
			semconv.SchemaURL,
			semconv.ServiceNameKey.String(serviceName),
			semconv.ServiceVersionKey.String(serviceVersion),
			attribute.String("environment", getEnvironment()),
		),
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create resource: %w", err)
	}

	// Create trace provider
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithBatcher(exp),
		sdktrace.WithResource(res),
		sdktrace.WithSampler(sdktrace.TraceIDRatioBased(config.SamplingRate)),
	)

	// Set global tracer provider
	otel.SetTracerProvider(tp)

	log.Printf("📊 Tracing enabled (OTLP: %s, sampling: %.2f%%)\n",
		config.OTLPEndpoint, config.SamplingRate*100)

	// Return shutdown function
	return tp.Shutdown, nil
}

// TracingMiddleware adds OpenTelemetry tracing to Gin requests
func TracingMiddleware(tracerName string) gin.HandlerFunc {
	tracer := otel.Tracer(tracerName)

	return func(c *gin.Context) {
		// Skip health and metrics endpoints
		if c.Request.URL.Path == "/health" || c.Request.URL.Path == "/metrics" {
			c.Next()
			return
		}

		// Extract context from incoming request (for distributed tracing)
		ctx := otel.GetTextMapPropagator().Extract(
			c.Request.Context(),
			&ginCarrier{c: c},
		)

		// Start span
		spanName := fmt.Sprintf("%s %s", c.Request.Method, c.Request.URL.Path)
		ctx, span := tracer.Start(ctx, spanName,
			trace.WithSpanKind(trace.SpanKindServer),
			trace.WithAttributes(
				semconv.HTTPMethodKey.String(c.Request.Method),
				semconv.HTTPRouteKey.String(c.FullPath()),
				semconv.HTTPURLKey.String(c.Request.URL.String()),
				semconv.HTTPUserAgentKey.String(c.Request.UserAgent()),
				semconv.HTTPClientIPKey.String(c.ClientIP()),
			),
		)
		defer span.End()

		// Store span in context for child spans
		c.Request = c.Request.WithContext(ctx)
		c.Set("tracer", tracer)
		c.Set("span", span)

		// Extract auth context and add to span
		if authCtx, exists := c.Get("auth"); exists {
			if auth, ok := authCtx.(*AuthContext); ok {
				span.SetAttributes(
					attribute.String("user.id", auth.UserID),
					attribute.String("user.name", auth.Username),
					attribute.String("tenant.id", auth.TenantID),
					attribute.String("user.role", string(auth.Role)),
					attribute.Bool("is_api_key", auth.IsAPIKey),
				)
			}
		}

		start := time.Now()

		// Process request
		c.Next()

		// Record response attributes
		duration := time.Since(start)
		statusCode := c.Writer.Status()

		span.SetAttributes(
			semconv.HTTPStatusCodeKey.Int(statusCode),
			attribute.Int64("http.response_size", int64(c.Writer.Size())),
			attribute.Float64("http.duration_ms", float64(duration.Milliseconds())),
		)

		// Mark span as error if status code >= 400
		if statusCode >= 400 {
			span.SetAttributes(attribute.Bool("error", true))
			if len(c.Errors) > 0 {
				span.SetAttributes(attribute.String("error.message", c.Errors.String()))
			}
		}

		// Add custom events for notable actions
		if statusCode >= 500 {
			span.AddEvent("server_error", trace.WithAttributes(
				attribute.Int("status_code", statusCode),
			))
		}
	}
}

// ginCarrier is a carrier for extracting trace context from Gin context
type ginCarrier struct {
	c *gin.Context
}

func (g *ginCarrier) Get(key string) string {
	return g.c.GetHeader(key)
}

func (g *ginCarrier) Set(key, value string) {
	g.c.Header(key, value)
}

func (g *ginCarrier) Keys() []string {
	keys := make([]string, 0, len(g.c.Request.Header))
	for key := range g.c.Request.Header {
		keys = append(keys, key)
	}
	return keys
}

// StartSpan starts a new child span from the current context
func StartSpan(c *gin.Context, name string, attrs ...attribute.KeyValue) (context.Context, trace.Span) {
	tracer := otel.Tracer(serviceName)
	ctx := c.Request.Context()

	ctx, span := tracer.Start(ctx, name,
		trace.WithAttributes(attrs...),
	)

	return ctx, span
}

// RecordError records an error on the current span
func RecordError(span trace.Span, err error) {
	if err != nil && span != nil {
		span.RecordError(err)
		span.SetAttributes(attribute.Bool("error", true))
	}
}

// AddEvent adds an event to the current span
func AddEvent(span trace.Span, name string, attrs ...attribute.KeyValue) {
	if span != nil {
		span.AddEvent(name, trace.WithAttributes(attrs...))
	}
}

// getEnvironment returns the current environment
func getEnvironment() string {
	env := getEnvVar("ENVIRONMENT", "development")
	return env
}

// Helper function to get environment variable with default
func getEnvVar(key, defaultValue string) string {
	value := os.Getenv(key)
	if value != "" {
		return value
	}
	return defaultValue
}

// TraceHTTPRequest wraps an HTTP request to core service with tracing
func (cp *ControlPlane) TraceHTTPRequest(c *gin.Context, method, path string) (interface{}, error) {
	ctx, span := StartSpan(c, fmt.Sprintf("HTTP %s %s", method, path),
		attribute.String("http.method", method),
		attribute.String("http.url", CoreServiceURL+path),
		attribute.String("span.kind", "client"),
	)
	defer span.End()

	token, _ := ExtractToken(c) //nolint:errcheck // empty token is acceptable

	// Inject trace context into outgoing request
	carrier := make(map[string]string)
	otel.GetTextMapPropagator().Inject(ctx, &mapCarrier{m: carrier})

	// Build request with trace headers
	req := cp.client.R().
		SetHeader("Authorization", "Bearer "+token)

	// Add trace context headers
	for k, v := range carrier {
		req.SetHeader(k, v)
	}

	// Execute request
	resp, err := req.Execute(method, path)

	// Record in span
	if err != nil {
		RecordError(span, err)
		return nil, err
	}

	span.SetAttributes(
		attribute.Int("http.status_code", resp.StatusCode()),
		attribute.Int("http.response_size", len(resp.Body())),
	)

	if resp.StatusCode() >= 400 {
		span.SetAttributes(attribute.Bool("error", true))
	}

	return resp, nil
}

// mapCarrier is a simple map-based carrier for trace context
type mapCarrier struct {
	m map[string]string
}

func (m *mapCarrier) Get(key string) string {
	return m.m[key]
}

func (m *mapCarrier) Set(key, value string) {
	m.m[key] = value
}

func (m *mapCarrier) Keys() []string {
	keys := make([]string, 0, len(m.m))
	for k := range m.m {
		keys = append(keys, k)
	}
	return keys
}
