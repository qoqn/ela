# Minimal XFCE desktop over xrdp, user ela / ela.
FROM debian:trixie-slim
# Override with a nearby mirror for faster builds, e.g.
#   DEBIAN_MIRROR=http://kartolo.sby.datautama.net.id/debian docker compose build rdp
ARG DEBIAN_MIRROR=http://deb.debian.org/debian
RUN sed -i "s|^URIs: http://deb.debian.org/debian$|URIs: ${DEBIAN_MIRROR}|" /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      xrdp xorgxrdp xfce4 xfce4-terminal dbus-x11 ssl-cert xclip \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -m -s /bin/bash ela && echo 'ela:ela' | chpasswd \
 && adduser xrdp ssl-cert \
 && echo 'startxfce4' > /home/ela/.xsession && chown ela:ela /home/ela/.xsession
EXPOSE 3389
CMD ["/bin/sh", "-c", "rm -f /var/run/xrdp/*.pid; /usr/sbin/xrdp-sesman && exec /usr/sbin/xrdp --nodaemon"]
