# Design

This is reconciliation, not a marketplace. Source and destination are always explicit.
The normalized inventory identifies comparable items but preserves original client fields.

Discovery never writes. Preview computes destination content and hash. Apply refuses if
the hash changed, writes a backup/receipt, then atomically replaces. Undo also hash-checks.
Secret values remain references when possible and are redacted from UI/logs/receipts.

Writers ship one client at a time after byte-preservation fixtures for comments and
unknown fields.
