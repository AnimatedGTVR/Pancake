#!/usr/bin/env bash
# shellcheck disable=SC2034

iso_name="pancake"
iso_label="PANCAKE_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Pancake DE"
iso_application="Pancake Desktop Environment Live"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
buildmodes=('iso')
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'zstd' '-b' '1M' '-Xcompression-level' '15')
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/root"]="0:0:750"
  ["/usr/local/bin/pancake"]="0:0:755"
  ["/usr/local/bin/start-pancake"]="0:0:755"
  ["/usr/local/bin/start-pancake-terminal"]="0:0:755"
  ["/root/.bash_profile"]="0:0:644"
  ["/root/.bashrc"]="0:0:644"
)
