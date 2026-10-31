#!/bin/sh
# If a public key file is mounted at /keys/authorized_keys, allow it for the test user.
set -e
if [ -f /keys/authorized_keys ]; then
  install -d -m 700 -o tester -g tester /home/tester/.ssh
  install -m 600 -o tester -g tester /keys/authorized_keys /home/tester/.ssh/authorized_keys
fi
exec /usr/sbin/sshd -D -e
