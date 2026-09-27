# Customer review work bounds

Continue `t-7ce02d` and `t-7b4430` through the existing evidence workflow.
Metering receipts prevent duplicate charges; they do not limit concurrent
retries or cancel the work that those retries perform.

## Two resource boundaries

Every run read uses the fixed Core leader and `retained-entity-v1`. Core's
existing archive pool owns both cold preparation and retained-entity scanning.
Its worker keeps the permit after an HTTP timeout or disconnect until blocking
work exits. All Query Service replicas using that leader therefore share that
database-work bound. Preserve this mechanism; do not add another database or
weaken the strict read contract.

Query Service also needs its own bound for workflow memory, comparison and
metadata calls. Add a dedicated supervised pool with four active workflows per
instance and two per tenant. Refuse excess work immediately, without reserving
quota, fetching a source or creating a waiting job. The existing 20-second
workflow deadline starts when admitted. Source sharing, preparation and live
review/result reads must all enter this pool before performing workflow work.

Each admitted worker has a caller monitor and a deadline. Caller departure or
deadline kills the worker. Capacity is released only after its DOWN message,
not when cancellation is requested. A one-for-all subtree owns the task
supervisor and dispatcher: loss of either stops outstanding workers before a
new dispatcher admits work. Errors are fixed atoms; dispatcher crash formatting
must not expose closures, tenant identifiers, credentials or result bodies.

## Alternatives and limits

A process-local semaphore without caller monitors leaves abandoned work and
restart gaps. A durable lease with client-chosen expiry can admit a replacement
while an old process or kernel operation still runs. Neither improves on Core's
existing owner-held worker permits. Use that shared database boundary plus the
explicit per-instance workflow pool. This is not a cluster-wide cap of four
whole workflows: adding Query Service instances increases their aggregate
comparison capacity, while all retained source I/O still uses the leader pool.

Read cancellation may follow a committed metering receipt or pending draft.
Retries preserve the original request ID and period. No refund, execution,
human approval or automatic retry is added. Core archive warming can outlive
the gateway caller but retains its existing worker permit and deadline.

## Required proof

Exercise tenant/global exhaustion, immediate refusal without work, capacity
recovery, deadline cancellation, normal and abnormal caller departure, worker
failure, dispatcher/task-supervisor restart and independently supervised pools.
Through actual Core, prove busy admission performs no charge/source read and
an interrupted admitted read can retry without a second charge or lost draft.
Keep strict full Query Service checks and the actual Core workflow regressions.

The public source/MCP bindings, billing-period transition, product human gate,
display and actual host journeys remain required. This change activates none
of those surfaces and does not by itself complete an access or tool bead.
