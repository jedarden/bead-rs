# Structured data CLI input transport v1

Owner: beadrs-055c04ed. This independently authored native extension supports
large durable outcome checkpoints for shephrd-190d3302 without placing their
JSON bodies in process arguments.

`bead data set` requires exactly one of `--value JSON` and `--value-file PATH`.
The existing literal form retains its behavior. A file path is relative to the
invocation directory; `--value-file -` reads standard input through EOF. Input
must be UTF-8 JSON, and JSON whitespace is accepted without rewriting it before
the existing CLI secret preflight. Files and stdin are read once before that
preflight and before acquiring workspace mutation locks. The scanned bytes and
the bytes subsequently parsed for the service must be identical.

There is no command-argument size limit on file/stdin payloads. Both routes use
the same JSON parser, service validations, secret policy, audit events, and
automatic checkpoint publication as literal input. Invalid input must not
change issue data or append mutation events. Neither successful set output nor
failure diagnostics echo the payload. Transport errors report an I/O failure;
JSON errors retain the existing validation error behavior.

Acceptance includes file and stdin round trips over 128 KiB, required/exclusive
input arguments, unreadable or non-UTF-8 input, invalid JSON, service validation
failure, and atomic redacted secret rejection, including secrets at the end of
a large input. No storage format or schema changes are part of this extension.
