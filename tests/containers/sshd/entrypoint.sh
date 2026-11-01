#!/bin/sh
# If a public key file is mounted at /keys/authorized_keys, allow it for the test user.
set -e
if [ -f /keys/authorized_keys ]; then
  install -d -m 700 -o tester -g tester /home/tester/.ssh
  install -m 600 -o tester -g tester /keys/authorized_keys /home/tester/.ssh/authorized_keys
fi
# If the host's Docker socket is mounted, let the test user use it the way a member of the host's
# docker group would.
if [ -S /var/run/docker.sock ]; then
  gid=$(stat -c %g /var/run/docker.sock)
  getent group "$gid" >/dev/null || groupadd -g "$gid" dockersock
  usermod -aG "$gid" tester
fi
exec /usr/sbin/sshd -D -e
