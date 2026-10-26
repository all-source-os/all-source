package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	allsource "github.com/all-source-os/allsource-go"
)

var errStop = errors.New("stop after the snapshot")

func report(fields map[string]any) {
	fields["sdk"] = "go"
	line, _ := json.Marshal(fields)
	fmt.Println(string(line))
}

func main() {
	state := os.Getenv("PR366_STATE")
	if state == "" {
		state = filepath.Join(os.TempDir(), "pr366-proof")
	}
	raw, err := os.ReadFile(filepath.Join(state, "e2e-secrets.json"))
	if err != nil {
		panic(err)
	}
	var secrets struct {
		A string `json:"a"`
	}
	if err := json.Unmarshal(raw, &secrets); err != nil {
		panic(err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	client := allsource.New(secrets.A, "http://127.0.0.1:4901")
	key := "probe-a:" + time.Now().UTC().Format("2006-01-02")

	row, err := client.Latest(ctx, "ops_status", key, allsource.LatestOptions{})
	if err != nil {
		report(map[string]any{"call": "Latest", "ok": false, "error": err.Error()})
		os.Exit(1)
	}
	report(map[string]any{"call": "Latest", "ok": true, "state": row.State, "etag": row.ETag})

	var keys []string
	for entity, err := range client.LatestMany(ctx, "ops_status", allsource.LatestManyOptions{Prefix: "probe-"}) {
		if err != nil {
			report(map[string]any{"call": "LatestMany", "ok": false, "error": err.Error()})
			os.Exit(1)
		}
		keys = append(keys, entity.Key)
	}
	report(map[string]any{"call": "LatestMany", "ok": true, "keys": keys})

	var upserts int
	var checkpoint int64
	err = client.Sync(ctx, "ops_status", allsource.SyncOptions{}, func(batch allsource.SyncBatch) error {
		upserts += len(batch.Upserts)
		if batch.Snapshotting {
			return nil
		}
		checkpoint = batch.Checkpoint
		return errStop
	})
	if !errors.Is(err, errStop) {
		report(map[string]any{"call": "Sync", "ok": false, "error": fmt.Sprint(err)})
		os.Exit(1)
	}
	report(map[string]any{"call": "Sync", "ok": true, "snapshot_upserts": upserts, "checkpoint": checkpoint})
}
