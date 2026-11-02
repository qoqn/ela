# An SSH server that only offers keyboard-interactive, through PAM.
# ela / ela signs in with the password alone; otp / otp is also asked for a TOTP code
# from the fixed secret below, which the tests compute.
FROM debian:trixie-slim
ARG DEBIAN_MIRROR=http://deb.debian.org/debian
RUN sed -i "s|^URIs: http://deb.debian.org/debian$|URIs: ${DEBIAN_MIRROR}|" /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      openssh-server libpam-google-authenticator \
 && rm -rf /var/lib/apt/lists/* /etc/ssh/ssh_host_* \
 && mkdir -p /run/sshd /etc/ssh/keys \
 && useradd -m -s /bin/bash ela && echo 'ela:ela' | chpasswd \
 && useradd -m -s /bin/bash otp && echo 'otp:otp' | chpasswd \
 && printf 'JBSWY3DPEHPK3PXP\n" TOTP_AUTH\n' > /home/otp/.google_authenticator \
 && chown otp:otp /home/otp/.google_authenticator && chmod 400 /home/otp/.google_authenticator \
 && echo 'auth required pam_google_authenticator.so nullok' >> /etc/pam.d/sshd
COPY sshd_config /etc/ssh/sshd_config.d/only-keyboard-interactive.conf
EXPOSE 22
# Host keys live in a volume, so known_hosts stays valid across re-creation.
CMD ["/bin/sh", "-c", "for t in ed25519 rsa; do [ -f /etc/ssh/keys/ssh_host_${t}_key ] || ssh-keygen -q -t $t -N '' -f /etc/ssh/keys/ssh_host_${t}_key; done; exec /usr/sbin/sshd -D -e"]
