# Configuration

The VM can be configured through a variety of environment variables. Such
settings include the number of threads to use, how often to collect garbage, and
more.

## Available variables

| Variable                   | Default   | Purpose
|:---------------------------|:----------|:-------------------------------------
| INKO_PRIMARY_THREADS       | CPU cores | The number of threads for running processes.
| INKO_BLOCKING_THREADS      | CPU cores | The number of threads for blocking processes.
| INKO_GC_THREADS            | CPU cores | The number of GC coordination threads.
| INKO_TRACER_THREADS        | CPU cores | The number of threads for parallel tracing during garbage collection.
| INKO_BYTECODE_THREADS      | CPU cores | The number of threads to use for parsing bytecode.
| INKO_REDUCTIONS            | 1000      | The number of reductions before a process is suspended.
| INKO_YOUNG_THRESHOLD       | 256       | The number of blocks to allocate before triggering a young collection.
| INKO_MATURE_THRESHOLD      | 512       | The number of blocks to allocate before triggering a full collection.
| INKO_HEAP_GROWTH_FACTOR    | 1.5       | The factor to grow the heap by if not enough memory could be garbage collected.
| INKO_HEAP_GROWTH_THRESHOLD | 0.9       | The percentage of the heap (0% being 0.0 and 100% being 1.0) that needs to remain in use before growing it.
| INKO_PRINT_GC_TIMINGS      | false     | Prints GC collection timings to STDERR.

Here "CPU cores" means the number of logical CPU cores.

The number of bytecode threads is limited to a maximum of 4 threads. So if you
have 12 CPU cores, only 4 will be used. But if you have 3 CPU cores, all 3 will
be used.
