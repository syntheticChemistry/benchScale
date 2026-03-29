#!/usr/bin/env bash
# Build Ubuntu 24.04 with COSMIC Desktop + RustDesk
# Uses Ubuntu cloud image as base + COSMIC packages from System76

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="/tmp/ubuntu-cosmic-build-$(date +%Y%m%d-%H%M%S).log"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Ubuntu 24.04 + COSMIC Desktop + RustDesk Pipeline                      ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "This pipeline creates Ubuntu 24.04 with:"
echo "  - COSMIC desktop from System76 repository"
echo "  - RustDesk for remote access"
echo "  - Fully automated and reproducible"
echo ""
echo "Log: ${LOG_FILE}"
echo ""

# Use existing Ubuntu cloud image
BASE_IMAGE="../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img"

if [ ! -f "${BASE_IMAGE}" ]; then
    echo "❌ Ubuntu cloud image not found: ${BASE_IMAGE}"
    echo "   Run: cd ../agentReagents/scripts && ./download-cloud-images.sh"
    exit 1
fi

echo "✅ Base image found: ${BASE_IMAGE}"
echo ""

# VM name
VM_NAME="ubuntu-cosmic-rustdesk-$(date +%Y%m%d-%H%M%S)"

echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Creating VM and Installing COSMIC                                      ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "VM: ${VM_NAME}"
echo "This will take ~20 minutes for full installation"
echo ""

# Create VM using benchScale (we have this working!)
cd "${SCRIPT_DIR}/../ionChannel"
cargo run --example create_test_vm --features benchscale -- "${VM_NAME}" 2>&1 | tee -a "${LOG_FILE}"

if [ $? -ne 0 ]; then
    echo "❌ VM creation failed"
    exit 1
fi

echo ""
echo "✅ VM created, getting IP..."

# Get VM IP
VM_IP=$(sudo virsh domifaddr "${VM_NAME}" | grep ipv4 | awk '{print $4}' | cut -d/ -f1)

if [ -z "$VM_IP" ]; then
    echo "⚠️  Getting IP from virsh output..."
    sleep 5
    VM_IP=$(sudo virsh domifaddr "${VM_NAME}" | grep ipv4 | awk '{print $4}' | cut -d/ -f1)
fi

echo "VM IP: ${VM_IP}"
echo ""

# Wait for SSH
echo "Waiting for SSH..."
for i in {1..60}; do
    if ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=2 ubuntu@${VM_IP} "echo connected" 2>/dev/null; then
        echo "✅ SSH ready"
        break
    fi
    if [ $i -eq 60 ]; then
        echo "❌ SSH timeout"
        exit 1
    fi
    sleep 2
done

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Installing COSMIC Desktop                                               ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""

# Install COSMIC and RustDesk
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@${VM_IP} <<'INSTALL'
set -e

echo "Step 1/5: Updating system..."
sudo apt-get update

echo "Step 2/5: Adding System76 repository for COSMIC..."
# Add System76 repository
sudo apt-get install -y software-properties-common
sudo add-apt-repository -y ppa:system76/pop
sudo apt-get update

echo "Step 3/5: Installing COSMIC desktop (this takes ~15 minutes)..."
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    cosmic-desktop \
    cosmic-session \
    cosmic-greeter \
    pipewire \
    wireplumber

echo "Step 4/5: Setting graphical target..."
sudo systemctl set-default graphical.target

echo "Step 5/5: Installing RustDesk..."
cd /tmp
wget -q https://github.com/rustdesk/rustdesk/releases/download/1.2.3/rustdesk-1.2.3-x86_64.deb
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

rm -f /tmp/rustdesk-1.2.3-x86_64.deb

echo ""
echo "✅ Installation complete!"
echo ""
echo "Installed components:"
echo "  - COSMIC Desktop (System76)"
echo "  - PipeWire audio"
echo "  - RustDesk $(rustdesk --version 2>/dev/null || echo '1.2.3')"
INSTALL

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  Rebooting to Start COSMIC Desktop                                      ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""

# Reboot
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@${VM_IP} "sudo reboot" || true

echo "⏳ Waiting 60 seconds for reboot..."
sleep 60

# Get VNC info
VNC_DISPLAY=$(sudo virsh vncdisplay "${VM_NAME}" 2>/dev/null || echo ":0")
VNC_PORT=$((5900 + ${VNC_DISPLAY#:}))

echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║  ✅ Ubuntu COSMIC + RustDesk Complete!                                   ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "VM: ${VM_NAME}"
echo "IP: ${VM_IP}"
echo "VNC: vncviewer localhost:${VNC_PORT}"
echo ""
echo "Login credentials:"
echo "  Username: ubuntu"
echo "  Password: (use SSH key)"
echo ""
echo "Next steps:"
echo "  1. Connect via VNC: vncviewer localhost:${VNC_PORT}"
echo "  2. Log into COSMIC desktop"
echo "  3. RustDesk will auto-start"
echo "  4. Note RustDesk ID for remote testing"
echo ""
echo "To save as template:"
echo "  sudo virsh shutdown ${VM_NAME}"
echo "  sudo cp /var/lib/libvirt/images/${VM_NAME}.qcow2 \\"
echo "         /var/lib/libvirt/images/ubuntu-cosmic-rustdesk-template.qcow2"
echo ""
echo "Build log: ${LOG_FILE}"
echo ""

