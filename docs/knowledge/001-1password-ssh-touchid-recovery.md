# 1Password SSH TouchID Recovery

## Symptoms

- `ssh-add -l` works fine (key listing succeeds)
- SSH signing fails with `agent refused operation`
- TouchID dialog does not appear; instead a "Confirm" button dialog shows, or nothing at all
- Restarting 1Password or rebooting does not fix the issue

## Log Diagnosis

Log path: `~/Library/Group Containers/2BUA8C4S2C.com.1password/Library/Application Support/1Password/Data/logs/1Password_rCURRENT.log`

Look for these errors:

```
WARN  [1P:op-settings/src/store/generic_entry.rs] waiting for setting unlock event timed out
ERROR [1P:ssh/op-ssh-agent/src/lib.rs] unable to obtain setting state, dropping signature request
INFO  [1P:ssh/op-ssh-config/src/lib.rs] agent not configured
```

The settings store unlock state is broken, preventing the SSH agent from reading its configuration.

## Recovery

1. In 1Password app menu, select **"Lock 1Password"**
2. Unlock with **master password** (not TouchID)
3. Verify with signing test:
   ```bash
   SSH_AUTH_SOCK="/Users/kawaz/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock" \
     ssh-keygen -Y sign -f <pubkey-file> -n git <any-file>
   ```

This restores TouchID functionality.

## Notes

- Observed on: 1Password 8.12.5 (BETA), macOS 26.3 (Tahoe) — 2026-03-04
- authsock-filter is not involved (reproduced with direct 1Password socket)
- Trigger is unknown (repeated authentication failures suspected but unconfirmed)
