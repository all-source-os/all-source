defmodule QueryServiceEx.Views.ReducerTest do
  use ExUnit.Case, async: true

  alias QueryServiceEx.Views.Reducer

  @ops_status %{
    "source" => %{"prefixes" => ["ops.probe."]},
    "key" => "{payload.probe}:{day:timestamp}",
    "ops" => [
      %{
        "on" => "ops.probe.observed",
        "set" => %{"probe" => "payload.probe", "day" => "day:timestamp"}
      },
      %{
        "on" => "ops.probe.observed",
        "when" => %{"payload.ok" => true},
        "increment" => %{"ok" => 1}
      },
      %{
        "on" => "ops.probe.observed",
        "when" => %{"payload.ok" => false},
        "increment" => %{"failed" => 1}
      },
      %{"on" => "ops.probe.observed", "max" => %{"last_seen_at" => "timestamp"}},
      %{"on" => "ops.probe.observed", "set" => %{"status" => "payload.status"}}
    ]
  }

  @lifecycle %{
    "source" => %{"prefixes" => ["skill."]},
    "key" => "{entity_id}",
    "ops" => [
      %{
        "on" => "skill.",
        "status" => %{
          "field" => "status",
          "from" => "suffix",
          "transitions" => %{"drafted" => ["published"], "published" => ["retired"]}
        }
      }
    ]
  }

  defp event(type, payload, opts \\ []) do
    %{
      "id" => "evt-#{System.unique_integer([:positive])}",
      "event_type" => type,
      "entity_id" => Keyword.get(opts, :entity_id, "probe:a"),
      "timestamp" => Keyword.get(opts, :at, "2026-10-27T10:00:00Z"),
      "payload" => payload
    }
  end

  defp view(ops, key \\ "{entity_id}", prefixes \\ ["t."]) do
    %{"source" => %{"prefixes" => prefixes}, "key" => key, "ops" => ops}
  end

  defp fold_all(definition, events, row \\ nil) do
    Enum.reduce(events, row, fn event, row ->
      case Reducer.apply(definition, row, event) do
        {:upsert, _key, next} -> next
        {:delete, _key} -> nil
        {:skip, _reason} -> row
      end
    end)
  end

  describe "the ops_status example" do
    test "folds observations into one row per probe and day" do
      payload = %{"probe" => "probe-a", "ok" => true, "status" => "green"}
      ok = event("ops.probe.observed", payload)

      assert {:upsert, "probe-a:2026-10-27", row} = Reducer.apply(@ops_status, nil, ok)

      assert row == %{
               "probe" => "probe-a",
               "day" => "2026-10-27",
               "ok" => 1,
               "last_seen_at" => "2026-10-27T10:00:00Z",
               "status" => "green"
             }

      payload = %{"probe" => "probe-a", "ok" => false, "status" => "red"}
      failed = event("ops.probe.observed", payload, at: "2026-10-27T09:00:00Z")

      assert {:upsert, "probe-a:2026-10-27", row} = Reducer.apply(@ops_status, row, failed)
      assert row["ok"] == 1
      assert row["failed"] == 1
      assert row["last_seen_at"] == "2026-10-27T10:00:00Z"
      assert row["status"] == "red"
    end

    test "a different UTC day is a different row" do
      payload = %{"probe" => "probe-a", "ok" => true}
      late = event("ops.probe.observed", payload, at: "2026-10-27T23:30:00-02:00")

      assert {:ok, "probe-a:2026-10-28"} = Reducer.key(@ops_status, late)
    end

    test "an event without the key's payload field is skipped" do
      missing = event("ops.probe.observed", %{"ok" => true})

      assert {:skip, :missing_key_field} = Reducer.apply(@ops_status, nil, missing)
    end

    test "an event outside the source prefixes is skipped" do
      other = event("billing.charged", %{"probe" => "probe-a"})

      assert {:skip, :outside_source} = Reducer.apply(@ops_status, nil, other)
    end
  end

  describe "key template" do
    test "renders entity_id, event_type and literal text" do
      ops = [%{"on" => "t.a", "set" => %{"x" => %{"literal" => 1}}}]
      definition = view(ops, "row/{event_type}/{entity_id}")

      assert {:ok, "row/t.a/e-1"} = Reducer.key(definition, event("t.a", %{}, entity_id: "e-1"))
    end

    test "renders scalar payload values as text" do
      definition = view([%{"on" => "t.a", "set" => %{}}], "{payload.n}-{payload.flag}")

      assert {:ok, "7-true"} = Reducer.key(definition, event("t.a", %{"n" => 7, "flag" => true}))
    end

    test "a non-scalar payload value cannot be a key" do
      definition = view([%{"on" => "t.a", "set" => %{}}], "{payload.obj}")

      assert {:skip, :missing_key_field} =
               Reducer.key(definition, event("t.a", %{"obj" => %{"a" => 1}}))
    end

    test "a key over 512 bytes is skipped" do
      definition = view([%{"on" => "t.a", "set" => %{}}], "{payload.id}")
      long = String.duplicate("k", 513)

      assert {:skip, :key_too_long} = Reducer.key(definition, event("t.a", %{"id" => long}))
    end
  end

  describe "on and when" do
    test "on matches an exact type, a prefix ending in a dot and a prefix ending in *" do
      exact = view([%{"on" => "t.a", "increment" => %{"n" => 1}}])
      dotted = view([%{"on" => "t.", "increment" => %{"n" => 1}}])
      star = view([%{"on" => "t.a*", "increment" => %{"n" => 1}}])

      assert {:upsert, _, %{"n" => 1}} = Reducer.apply(exact, nil, event("t.a", %{}))
      assert {:skip, :no_matching_op} = Reducer.apply(exact, nil, event("t.ab", %{}))
      assert {:upsert, _, %{"n" => 1}} = Reducer.apply(dotted, nil, event("t.anything", %{}))
      assert {:upsert, _, %{"n" => 1}} = Reducer.apply(star, nil, event("t.abc", %{}))
      assert {:skip, :no_matching_op} = Reducer.apply(star, nil, event("t.b", %{}))
    end

    test "when requires every listed path to equal its scalar" do
      op = %{
        "on" => "t.a",
        "when" => %{"payload.region" => "eu", "payload.live" => true},
        "increment" => %{"n" => 1}
      }

      definition = view([op])

      matching = event("t.a", %{"region" => "eu", "live" => true})
      partial = event("t.a", %{"region" => "eu", "live" => false})

      assert {:upsert, _, %{"n" => 1}} = Reducer.apply(definition, nil, matching)
      assert {:skip, :no_matching_op} = Reducer.apply(definition, nil, partial)
    end
  end

  describe "operations" do
    test "set takes the last writer and ignores a missing payload field" do
      definition = view([%{"on" => "t.a", "set" => %{"name" => "payload.name"}}])

      events = [event("t.a", %{"name" => "one"}), event("t.a", %{"name" => "two"})]
      row = fold_all(definition, events)
      assert row == %{"name" => "two"}

      assert {:upsert, _, %{"name" => "two"}} = Reducer.apply(definition, row, event("t.a", %{}))
    end

    test "set accepts literal, entity_id, event_type and timestamp expressions" do
      set = %{
        "kind" => %{"literal" => %{"nested" => [1, 2]}},
        "id" => "entity_id",
        "type" => "event_type",
        "at" => "timestamp"
      }

      definition = view([%{"on" => "t.a", "set" => set}])

      assert {:upsert, "e-9", row} =
               Reducer.apply(definition, nil, event("t.a", %{}, entity_id: "e-9"))

      assert row == %{
               "kind" => %{"nested" => [1, 2]},
               "id" => "e-9",
               "type" => "t.a",
               "at" => "2026-10-27T10:00:00Z"
             }
    end

    test "unset removes fields and delete removes the row" do
      ops = [
        %{"on" => "t.set", "set" => %{"a" => "payload.a", "b" => "payload.b"}},
        %{"on" => "t.unset", "unset" => ["b"]},
        %{"on" => "t.delete", "delete" => true}
      ]

      definition = view(ops)
      events = [event("t.set", %{"a" => 1, "b" => 2}), event("t.unset", %{})]
      row = fold_all(definition, events)
      assert row == %{"a" => 1}

      assert {:delete, "probe:a"} = Reducer.apply(definition, row, event("t.delete", %{}))
    end

    test "increment starts from 0 and adds integers" do
      definition = view([%{"on" => "t.a", "increment" => %{"n" => 2, "m" => -1}}])

      row = fold_all(definition, [event("t.a", %{}), event("t.a", %{})])
      assert row == %{"n" => 4, "m" => -2}
    end

    test "max and min compare numbers numerically" do
      op = %{"on" => "t.a", "max" => %{"hi" => "payload.v"}, "min" => %{"lo" => "payload.v"}}
      definition = view([op])

      row = fold_all(definition, Enum.map([5, 12, 3.5, 9], &event("t.a", %{"v" => &1})))
      assert row == %{"hi" => 12, "lo" => 3.5}
    end

    test "max and min compare RFC 3339 timestamps lexically" do
      op = %{"on" => "t.a", "max" => %{"last" => "timestamp"}, "min" => %{"first" => "timestamp"}}
      definition = view([op])

      stamps = ["2026-10-27T10:00:00Z", "2026-10-28T08:00:00Z", "2026-10-26T23:59:59Z"]
      events = Enum.map(stamps, &event("t.a", %{}, at: &1))

      assert fold_all(definition, events) == %{
               "last" => "2026-10-28T08:00:00Z",
               "first" => "2026-10-26T23:59:59Z"
             }
    end

    test "append and remove keep set semantics" do
      ops = [
        %{"on" => "t.add", "append" => %{"tags" => "payload.tag"}},
        %{"on" => "t.drop", "remove" => %{"tags" => "payload.tag"}}
      ]

      definition = view(ops)

      events = [
        event("t.add", %{"tag" => "a"}),
        event("t.add", %{"tag" => "b"}),
        event("t.add", %{"tag" => "a"}),
        event("t.drop", %{"tag" => "a"}),
        event("t.drop", %{"tag" => "z"})
      ]

      assert fold_all(definition, events) == %{"tags" => ["b"]}
    end

    test "remove on a missing field does not create it" do
      definition = view([%{"on" => "t.drop", "remove" => %{"tags" => "payload.tag"}}])

      drop = event("t.drop", %{"tag" => "a"})
      assert {:upsert, _, row} = Reducer.apply(definition, %{"x" => 1}, drop)
      assert row == %{"x" => 1}
    end
  end

  describe "status" do
    test "sets the field from the event type's last segment" do
      assert {:upsert, _, %{"status" => "drafted"}} =
               Reducer.apply(@lifecycle, nil, event("skill.drafted", %{}))
    end

    test "an unset field accepts any first value" do
      assert {:upsert, _, %{"status" => "retired"}} =
               Reducer.apply(@lifecycle, nil, event("skill.retired", %{}))
    end

    test "a listed transition is applied" do
      row = %{"status" => "drafted"}

      assert {:upsert, _, %{"status" => "published"}} =
               Reducer.apply(@lifecycle, row, event("skill.published", %{}))
    end

    test "an unlisted transition is ignored and counted" do
      row = %{"status" => "retired"}

      assert {{:upsert, _, ^row}, %{rejected_transition: 1}} =
               Reducer.fold(@lifecycle, row, event("skill.drafted", %{}))
    end

    test "a status from an expression without transitions always applies" do
      status = %{"field" => "state", "from" => "payload.state"}
      definition = view([%{"on" => "t.a", "status" => status}])

      events = [event("t.a", %{"state" => "x"}), event("t.a", %{"state" => "y"})]
      row = fold_all(definition, events)
      assert row == %{"state" => "y"}
    end
  end
end
