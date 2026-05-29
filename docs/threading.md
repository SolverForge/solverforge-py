# Threading

The primary target is CPython 3.14 free-threaded.

The native extension declares that it does not require the GIL. Rust worker
threads attach to Python only when invoking Python callbacks. User callback code
must be safe to run concurrently, or it must opt into serialized execution when
that option is exposed.

Third-party Python extension modules used inside callbacks may still impose
their own synchronization constraints.

