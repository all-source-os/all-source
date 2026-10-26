defmodule QueryServiceEx.Views.ViewFolderTest do
  use ExUnit.Case, async: false

  alias QueryServiceEx.Views.FakeCore
  alias QueryServiceEx.Views.ViewFolder

  @model "view_ops_status"
  @marker "view_Gops_status"
  @skipped [:query_service_ex, :views, :skipped]
  @rejected [:query_service_ex, :views, :rejected_transition]

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
      }
    ]
  }

  setup do
    start_supervised!(FakeCore)
    tenant = "tenant-#{System.unique_integer([:positive])}"
    {:ok, tenant: tenant}
  end

  defp probe(name, ok, payload \\ %{}) do
    %{
      "id" => "evt-#{System.unique_integer([:positive])}",
      "event_type" => "ops.probe.observed",
      "entity_id" => "probe:" <> name,
      "timestamp" => "2026-10-27T10:00:00Z",
      "payload" => Map.merge(%{"probe" => name, "ok" => ok}, payload)
    }
  end

  defp start_folder(tenant, opts \\ []) do
    defaults = [
      tenant_id: tenant,
      view: "ops_status",
      generation: 1,
      definition: @ops_status,
      core: FakeCore,
      auto: false,
      subscribe: false
    ]

    start_supervised!({ViewFolder, Keyword.merge(defaults, opts)})
  end

  defp rows, do: FakeCore.model(@model).rows

  defp view_commits do
    for {@model, commit} <- FakeCore.commits(), do: commit
  end

  defp seed_folded(rows, position, generation \\ 1) do
    checkpoint = %{"epoch" => 1, "position" => position}
    FakeCore.put_model(@model, %{revision: 4, checkpoint: checkpoint, ready: true, rows: rows})
    generation_row = %{"generation" => %{"generation" => generation}}
    FakeCore.put_model(@marker, %{revision: 1, checkpoint: checkpoint, rows: generation_row})
  end

  defp eventually(fun, attempts \\ 50) do
    if fun.() or attempts == 0 do
      assert fun.()
    else
      Process.sleep(20)
      eventually(fun, attempts - 1)
    end
  end

  test "folds the feed into rows and records the folded generation", %{tenant: tenant} do
    billing = %{probe("x", true) | "event_type" => "billing.charged"}
    FakeCore.add_events([probe("a", true), probe("a", false), billing, probe("b", true)])

    assert {:ok, status} = tenant |> start_folder() |> ViewFolder.sync()

    assert rows() == %{
             "a:2026-10-27" => %{"probe" => "a", "day" => "2026-10-27", "ok" => 1, "failed" => 1},
             "b:2026-10-27" => %{"probe" => "b", "day" => "2026-10-27", "ok" => 1}
           }

    assert [%{reset: true, expected_revision: 0, ready: true}] = view_commits()
    assert FakeCore.model(@model).checkpoint == %{"epoch" => 1, "position" => 4}
    assert FakeCore.model(@marker).rows == %{"generation" => %{"generation" => 1}}
    assert %{ready: true, revision: 1, rebuilding: false} = status
  end

  test "resumes from the stored checkpoint without refolding", %{tenant: tenant} do
    folded = %{"probe" => "a", "day" => "2026-10-27", "ok" => 5}
    seed_folded(%{"a:2026-10-27" => folded}, 3)
    FakeCore.add_events([probe("a", true), probe("a", true), probe("a", true), probe("a", true)])

    assert {:ok, _status} = tenant |> start_folder() |> ViewFolder.sync()

    assert rows()["a:2026-10-27"]["ok"] == 6
    assert [%{reset: false, expected_revision: 4}] = view_commits()
    assert FakeCore.get(& &1.row_reads) == 1
  end

  test "rebuilds from position 0 when the marker holds another generation", %{tenant: tenant} do
    seed_folded(%{"stale:row" => %{"ok" => 99}}, 1, 1)
    FakeCore.add_events([probe("a", true), probe("a", true)])

    assert {:ok, _status} = tenant |> start_folder(generation: 2) |> ViewFolder.sync()

    assert Map.keys(rows()) == ["a:2026-10-27"]
    assert rows()["a:2026-10-27"]["ok"] == 2
    assert [%{reset: true}] = view_commits()
    assert FakeCore.model(@marker).rows == %{"generation" => %{"generation" => 2}}
  end

  test "rebuilds when the feed epoch changed", %{tenant: tenant} do
    seed_folded(%{"a:2026-10-27" => %{"ok" => 1}}, 1)
    FakeCore.add_events([probe("a", true), probe("a", true), probe("b", true)])
    FakeCore.update(&%{&1 | epoch: 2})

    assert {:ok, _status} = tenant |> start_folder() |> ViewFolder.sync()

    assert rows()["a:2026-10-27"]["ok"] == 2
    assert rows()["b:2026-10-27"]["ok"] == 1
    assert [%{reset: true, checkpoint: %{"epoch" => 2, "position" => 3}}] = view_commits()
  end

  test "retries a commit whose conflict left the checkpoint unchanged", %{tenant: tenant} do
    seed_folded(%{"a:2026-10-27" => %{"ok" => 1}}, 1)
    FakeCore.add_events([probe("a", true), probe("a", true)])
    FakeCore.conflict_next(@model)

    assert {:ok, _status} = tenant |> start_folder() |> ViewFolder.sync()

    assert rows()["a:2026-10-27"]["ok"] == 2
    assert [%{expected_revision: 5}] = view_commits()
  end

  test "drops its batch when another writer already folded further", %{tenant: tenant} do
    seed_folded(%{}, 0)
    FakeCore.add_events([probe("a", true), probe("a", true)])
    theirs = %{"a:2026-10-27" => %{"ok" => 2}}

    FakeCore.conflict_next(@model, fn model ->
      %{model | checkpoint: %{"epoch" => 1, "position" => 2}, rows: theirs}
    end)

    assert {:ok, status} = tenant |> start_folder() |> ViewFolder.sync()

    assert rows() == theirs
    assert view_commits() == []
    assert status.checkpoint == %{"epoch" => 1, "position" => 2}
  end

  test "commits at most 500 changes per batch", %{tenant: tenant} do
    FakeCore.add_events(Enum.map(1..1_200, &probe("p#{&1}", true)))
    FakeCore.update(&%{&1 | page_size: 1_200})

    assert {:ok, _status} = tenant |> start_folder() |> ViewFolder.sync()

    sizes = Enum.map(view_commits(), &map_size(&1.changes))
    assert Enum.all?(sizes, &(&1 <= 500))
    assert Enum.sum(sizes) == 1_200
    assert map_size(rows()) == 1_200

    positions = Enum.map(view_commits(), & &1.checkpoint["position"])
    assert positions == Enum.sort(positions)
    assert List.last(positions) == 1_200
  end

  test "keeps every commit under 240 KiB", %{tenant: tenant} do
    blob = String.duplicate("x", 20_000)
    op = %{"on" => "ops.probe.observed", "set" => %{"blob" => "payload.blob"}}
    folder = start_folder(tenant, definition: %{@ops_status | "ops" => [op]})
    FakeCore.add_events(Enum.map(1..30, &probe("p#{&1}", true, %{"blob" => blob})))

    assert {:ok, _status} = ViewFolder.sync(folder)

    commits = view_commits()
    assert length(commits) > 1
    assert Enum.all?(commits, &(byte_size(Jason.encode!(&1)) <= 240 * 1024))
    assert map_size(rows()) == 30
  end

  test "reports a Core error and recovers on the next pass", %{tenant: tenant} do
    FakeCore.add_events([probe("a", true)])
    FakeCore.update(&%{&1 | fail_feed: 1})
    folder = start_folder(tenant)

    assert {:error, :core_unavailable} = ViewFolder.sync(folder)
    assert view_commits() == []

    assert {:ok, _status} = ViewFolder.sync(folder)
    assert rows()["a:2026-10-27"]["ok"] == 1
  end

  test "counts skipped events and rejected transitions", %{tenant: tenant} do
    test_pid = self()
    handler = "view-folder-test-#{tenant}"

    :telemetry.attach_many(
      handler,
      [@skipped, @rejected],
      fn event, measurements, metadata, _config ->
        send(test_pid, {:telemetry, event, measurements, metadata})
      end,
      nil
    )

    on_exit(fn -> :telemetry.detach(handler) end)

    status_op = %{
      "on" => "ops.probe.",
      "status" => %{
        "field" => "status",
        "from" => "suffix",
        "transitions" => %{"started" => ["finished"]}
      }
    }

    folder = start_folder(tenant, definition: %{@ops_status | "ops" => [status_op]})

    started = %{probe("a", true) | "event_type" => "ops.probe.started"}
    restarted = %{probe("a", true) | "event_type" => "ops.probe.restarted"}
    no_key = %{probe("a", true) | "payload" => %{}, "event_type" => "ops.probe.started"}
    FakeCore.add_events([started, restarted, no_key])

    assert {:ok, _status} = ViewFolder.sync(folder)

    assert_received {:telemetry, @rejected, %{count: 1}, %{tenant_id: ^tenant}}
    assert_received {:telemetry, @skipped, %{count: 1}, %{reason: :missing_key_field}}
    assert rows()["a:2026-10-27"]["status"] == "started"
  end

  test "wakes on a tenant event instead of waiting for the next poll", %{tenant: tenant} do
    folder = start_folder(tenant, auto: true, subscribe: true, idle_ms: 60_000)
    eventually(fn -> ViewFolder.status(folder).ready end)

    FakeCore.add_events([probe("a", true)])
    topic = "events:#{tenant}:all"
    Phoenix.PubSub.broadcast(QueryServiceEx.PubSub, topic, {:new_event, %{}})

    eventually(fn -> get_in(rows(), ["a:2026-10-27", "ok"]) == 1 end)
  end
end
