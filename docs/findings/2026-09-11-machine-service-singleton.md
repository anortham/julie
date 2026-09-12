# Machine service singleton admission

The machine service keeps its dynamic loopback port and now holds an exclusive OS lock on
`$JULIE_HOME/service.lock` for its full lifetime. Admission happens before binding a port or
constructing `ServiceApp`, so concurrent shims and direct `julie-server service` commands cannot
create competing service processes, workspace runtimes, or watchers.

`service.lock` is a zero-content runtime guard. It is never unlinked: deleting a locked path would
allow another process to lock a new inode. The operating system releases the lock when the service
exits or crashes, so there is no PID election or stale-lock cleanup.

This is an explicit exception to the earlier durable-root list of `registry.db`, `service.json`, and
`indexes/`. A fixed port was rejected because it creates unrelated port-collision and configuration
behavior and removes the existing dynamic-port discovery contract.
