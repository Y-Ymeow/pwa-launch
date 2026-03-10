#!/bin/bash
# 生成 Android 签名密钥

KEYSTORE_FILE="android.keystore"
KEY_ALIAS="upload"
KEYSTORE_PASSWORD="pwa-container-2024"
KEY_PASSWORD="pwa-container-2024"

# 检查是否已存在
if [ -f "$KEYSTORE_FILE" ]; then
    echo "Keystore 已存在: $KEYSTORE_FILE"
    exit 0
fi

# 生成密钥
echo "正在生成 Android 签名密钥..."
keytool -genkey \
    -v \
    -keystore "$KEYSTORE_FILE" \
    -alias "$KEY_ALIAS" \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -storepass "$KEYSTORE_PASSWORD" \
    -keypass "$KEY_PASSWORD" \
    -dname "CN=PWA Container, OU=Dev, O=PWA, L=City, ST=State, C=CN"

echo ""
echo "========================================"
echo "Keystore 生成成功: $KEYSTORE_FILE"
echo "别名: $KEY_ALIAS"
echo "密码: $KEYSTORE_PASSWORD"
echo "========================================"
echo ""
echo "在 GitHub 添加以下 Secrets:"
echo "  ANDROID_KEYSTORE_PASSWORD: $KEYSTORE_PASSWORD"
echo "  ANDROID_KEY_PASSWORD: $KEY_PASSWORD"
echo ""
echo "然后上传 keystore 到 GitHub Secrets (Base64):"
echo "  base64 -w 0 $KEYSTORE_FILE | xclip -selection clipboard"
