#!/usr/bin/env bash
# Post-install script for Pop!_OS COSMIC + RustDesk
# Run after OS installation is complete

set -e

if [ $# -ne 1 ]; then
    echo "Usage: $0 <vm-name>"
    exit 1
fi

VM_NAME="$1"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Pop!_OS COSMIC Post-Install: RustDesk Setup                            ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "VM: ${VM_NAME}"
echo ""

# Get VM IP
echo "Getting VM IP address..."
VM_IP=$(sudo virsh domifaddr "${VM_NAME}" | grep ipv4 | awk '{print $4}' | cut -d/ -f1)

if [ -z "$VM_IP" ]; then
    echo "❌ Could not get VM IP"
    echo "   Make sure VM is running and installation is complete"
    exit 1
fi

echo "✅ VM IP: ${VM_IP}"
echo ""

# Wait for SSH
echo "Waiting for SSH..."
for i in {1..30}; do
    if ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=2 cosmic@${VM_IP} "echo connected" 2>/dev/null; then
        echo "✅ SSH ready"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ SSH timeout"
        exit 1
    fi
    sleep 2
done

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Installing RustDesk                                                     ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""

# Install RustDesk
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null cosmic@${VM_IP} <<'INSTALL'
set -e

echo "Updating system..."
sudo apt-get update

echo "Installing dependencies..."
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y wget

echo "Downloading RustDesk..."
cd /tmp
wget -q https://github.com/rustdesk/rustdesk/releases/download/1.2.3/rustdesk-1.2.3-x86_64.deb

echo "Installing RustDesk..."
sudo DEBIAN_FRONTEND=noninteractive apt install -y -f ./rustdesk-1.2.3-x86_64.deb || true
sudo DEBIAN_FRONTEND=noninteractive apt install -y -f

echo "Configuring RustDesk autostart..."
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/rustdesk.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=RustDesk
Exec=/usr/bin/rustdesk
X-GNOME-Autostart-enabled=true
EOF

echo "✅ RustDesk installed and configured"

# Show system info
echo ""
echo "System information:"
echo "  OS: $(lsb_release -d | cut -f2)"
echo "  Desktop: COSMIC"
echo "  RustDesk: $(rustdesk --version 2>/dev/null || echo 'installed')"

rm -f /tmp/rustdesk-1.2.3-x86_64.deb
INSTALL

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  ✅ Post-Install Complete                                                ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Pop!_OS COSMIC with RustDesk is ready!"
echo ""
echo "VM: ${VM_NAME}"
echo "IP: ${VM_IP}"
echo "VNC: vncviewer localhost:$(sudo virsh vncdisplay ${VM_NAME} 2>/dev/null | sed 's/://' | awk '{print 5900 + $1}')"
echo ""
echo "Login credentials:"
echo "  Username: cosmic"
echo "  Password: cosmic2025"
echo ""
echo "Next steps:"
echo "  1. Connect via VNC"
echo "  2. Log into COSMIC desktop"
echo "  3. RustDesk will auto-start"
echo "  4. Note RustDesk ID for remote testing"
echo ""
echo "To save as template:"
echo "  sudo virsh shutdown ${VM_NAME}"
echo "  # Wait for shutdown"
echo "  sudo cp /var/lib/libvirt/images/${VM_NAME}.qcow2 /var/lib/libvirt/images/popos-cosmic-rustdesk-template.qcow2"
echo ""

