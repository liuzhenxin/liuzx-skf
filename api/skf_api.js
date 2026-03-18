/**
 * SKF Service Client Library
 * 
 * Provides a Promise-based interface for the SKF WebSocket Service.
 * 
 * Usage:
 *   const skf = new SKFClient('ws://127.0.0.1:9001');
 *   skf.on('connect', () => console.log('Connected!'));
 *   skf.connect();
 *   
 *   skf.enumProvider().then(providers => console.log(providers));
 */

class SKFClient {
    constructor(url = 'ws://127.0.0.1:9001') {
        this.url = url;
        this.ws = null;
        this.msgId = 1;
        this.callbacks = new Map();
        this.eventListeners = {
            connect: [],
            disconnect: [],
            error: []
        };
    }

    // Event Handling
    on(event, handler) {
        if (this.eventListeners[event]) {
            this.eventListeners[event].push(handler);
        }
    }

    emit(event, data) {
        if (this.eventListeners[event]) {
            this.eventListeners[event].forEach(handler => handler(data));
        }
    }

    connect() {
        return new Promise((resolve, reject) => {
            try {
                this.ws = new WebSocket(this.url);

                this.ws.onopen = () => {
                    this.emit('connect');
                    resolve();
                };

                this.ws.onclose = () => {
                    this.ws = null;
                    this.emit('disconnect');
                };

                this.ws.onerror = (err) => {
                    this.emit('error', err);
                    if (!this.isConnected()) reject(err);
                };

                this.ws.onmessage = (event) => {
                    try {
                        const data = JSON.parse(event.data);
                        if (data.id && this.callbacks.has(data.id)) {
                            const { resolve, reject } = this.callbacks.get(data.id);
                            this.callbacks.delete(data.id);
                            if (data.error) {
                                reject({ code: data.error, message: data.message || "Unknown Error" });
                            } else {
                                resolve(data.result);
                            }
                        }
                    } catch (e) {
                        console.error("SKFClient: Failed to parse message", e);
                    }
                };
            } catch (e) {
                reject(e);
            }
        });
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }

    isConnected() {
        return this.ws && this.ws.readyState === WebSocket.OPEN;
    }

    call(method, params = []) {
        return new Promise((resolve, reject) => {
            if (!this.isConnected()) {
                reject(new Error("Not connected to SKF Service"));
                return;
            }

            const id = this.msgId++;
            this.callbacks.set(id, { resolve, reject });

            const req = {
                jsonrpc: "2.0",
                method: method,
                params: params,
                id: id
            };

            this.ws.send(JSON.stringify(req));
        });
    }

    // --- High-Level API ---

    /**
     * Set the service language for error messages.
     * @param {string} lang "CN" or "EN"
     */
    async setLanguage(lang) {
        return this.call("SetLanguage", [lang]);
    }

    /**
     * Enumerate available providers.
     * @param {string} [vpid] Optional "VID:PID" filter
     * @returns {Promise<string[]>} List of provider names
     */
    async enumProvider(vpid) {
        const params = vpid ? [vpid] : [];
        const res = await this.call("EnumProvider", params);
        // Normalize result to array
        return Array.isArray(res) ? res : (res ? [res] : []);
    }

    /**
     * Enumerate devices for a specific provider.
     * @param {string} provider Provider name (from enumProvider)
     * @returns {Promise<string[]>} List of device names
     */
    async enumDevice(provider) {
        const res = await this.call("EnumDevice", [provider]);
        return Array.isArray(res) ? res : [];
    }

    /**
     * Block and wait for a device event (plug/unplug)
     * @param {string} provider Provider name
     * @returns {Promise<{deviceName: string, event: number}>} Event details (event 1=insert, 2=remove)
     */
    async waitForDevEvent(provider) {
        return this.call("WaitForDevEvent", [provider]);
    }

    /**
     * Enumerate applications on a specific device.
     * @param {string} provider Provider name
     * @param {string} deviceName Device name
     * @returns {Promise<string[]>} List of application names
     */
    async enumApplication(provider, deviceName) {
        const res = await this.call("EnumApplication", [provider, deviceName]);
        return Array.isArray(res) ? res : [];
    }

    /**
     * Enumerate containers in a specific application.
     * @param {string} provider Provider name
     * @param {string} deviceName Device name
     * @param {string} appName Application name
     * @returns {Promise<string[]>} List of container names
     */
    async enumContainer(provider, deviceName, appName) {
        const res = await this.call("EnumContainer", [provider, deviceName, appName]);
        return Array.isArray(res) ? res : [];
    }

    /**
     * Delete a container in a specific application.
     * @param {string} provider Provider name
     * @param {string} deviceName Device name
     * @param {string} appName Application name
     * @param {string} containerName Container name
     * @returns {Promise<boolean>} True if successful
     */
    async deleteContainer(provider, deviceName, appName, containerName) {
        return this.call("DeleteContainer", [provider, deviceName, appName, containerName]);
    }

    /**
     * Issue a certificate from a CSR (for testing).
     * @param {string} csr PEM or Base64 encoded PKCS#10 CSR
     * @param {boolean} [double=false] If true, generate double cert (sign + enc + ENVELOPEDKEYBLOB)
     * @returns {Promise<Object>} Object with certificate, certificate2?, encPriKey?, sessKey?, alg?, double
     */
    async issueCertificate(csr, double = false) {
        return this.call("IssueCertificate", [csr, double]);
    }

    /**
     * Import a certificate into a container.
     * @param {string} provider Provider name
     * @param {string} deviceName Device name
     * @param {string} appName Application name
     * @param {string} containerName Container name
     * @param {boolean} isSignCert True for signature certificate, false for encryption
     * @param {string} certData PEM or Base64 encoded certificate
     * @returns {Promise<boolean>} True if successful
     */
    async importCertificate(provider, deviceName, appName, containerName, isSignCert, certData) {
        return this.call("ImportCertificate", [provider, deviceName, appName, containerName, isSignCert, certData]);
    }

    /**
     * Import an encrypted key pair into the container.
     * @param {string} provider Provider name
     * @param {string} deviceName Device name
     * @param {string} appName Application name
     * @param {string} containerName Container name
     * @param {string} alg Algorithm ("SM2", "RSA")
     * @param {string} encKeyPair Base64 encoded encrypted key pair
     * @param {string} [wrapKey=""] Base64 encoded wrap key (RSA only)
     * @returns {Promise<boolean>} Success status
     */
    async importKeyPair(provider, deviceName, appName, containerName, alg, encKeyPair, wrapKey = "") {
        return this.call("ImportKeyPair", [provider, deviceName, appName, containerName, alg, encKeyPair, wrapKey]);
    }

    /**
     * Connect to a specific device.
     * @param {string} deviceName Device name (from enumDevice)
     * @returns {Promise<string|number>} Device handle
     */
    async connectDev(deviceName) {
        return this.call("ConnectDev", [deviceName]);
    }

    /**
     * Disconnect from a device.
     * @param {string|number} handle Device handle
     * @returns {Promise<boolean>} Success status
     */
    async disconnectDev(handle) {
        return this.call("DisConnectDev", [handle]);
    }

    /**
     * Generate random data from the device.
     * @param {string|number} handle Device handle
     * @param {number} length Length in bytes
     * @returns {Promise<string>} Base64 encoded random data
     */
    async generateRandom(handle, length) {
        return this.call("GenerateRandom", [handle, length]);
    }

    /**
     * Find certificates on all connected devices.
     * @param {string} filter "Sign", "Enc", or empty for all
     * @returns {Promise<Array<{key:string, value:string, type:string, cert:string}>>} List of certificates
     */
    async findCertificates(filter = "") {
        const res = await this.call("FindCertificates", [filter]);
        return Array.isArray(res) ? res : [];
    }

    /**
     * Sign data using the specified certificate container.
     * @param {string} certKey Certificate key path (provider/device/app/container/serial)
     * @param {string} dataBase64 Data to sign (base64 encoded)
     * @returns {Promise<string>} Base64-encoded signature (r || s)
     */
    async signData(certKey, dataBase64) {
        return this.call("SignData", [certKey, dataBase64]);
    }

    /**
     * Verify a PIN for a specific application.
     * @param {string} certKey Certificate key path (at least provider/device/app)
     * @param {string} pin User PIN
     * @returns {Promise<boolean>} True if PIN is correct
     */
    async checkPIN(certKey, pin) {
        return this.call("CheckPIN", [certKey, pin]);
    }

    /**
     * Compute a hash digest using the device.
     * @param {string} providerName Provider name
     * @param {string} deviceName Device name
     * @param {string} dataBase64 Data to hash (base64 encoded)
     * @param {string} [alg="SM3"] Algorithm: "SM3", "SHA1", "SHA256"
     * @returns {Promise<{hex:string, base64:string, algorithm:string, length:number}>}
     */
    async digest(providerName, deviceName, dataBase64, alg = "SM3") {
        return this.call("Digest", [providerName, deviceName, dataBase64, alg]);
    }

    /**
     * Create a PKCS#10 Certificate Signing Request.
     * @param {string} providerName Provider name
     * @param {string} deviceName Device name
     * @param {string} appName Application name
     * @param {string} subject Subject DN, e.g. "CN=Test,O=MyOrg,C=CN"
     * @param {string} [keyType="SM2"] Key type: "SM2" or "RSA"
     * @param {number} [keyLength=256] Key length in bits
     * @param {string} [containerName=""] Optional container name
     * @returns {Promise<{pem:string, container:string, keyType:string, keyLength:number}>}
     */
    async createPKCS10(providerName, deviceName, appName, subject, keyType = "SM2", keyLength = 256, containerName = "") {
        return this.call("CreatePKCS10", [providerName, deviceName, appName, subject, keyType, keyLength, containerName]);
    }
}

// Internationalization Support (API Messages Only)
SKFClient.translations = {
    'CN': {
        'msg_found_certs': '找到 {0} 张证书',
        'msg_sign_success': '签名成功',
        'msg_sign_failed': '签名失败',
        'msg_digest_success': '摘要计算成功',
        'msg_csr_success': 'CSR生成成功',
    },
    'EN': {
        'msg_found_certs': 'Found {0} certs',
        'msg_sign_success': 'Sign success',
        'msg_sign_failed': 'Sign failed',
        'msg_digest_success': 'Digest computed',
        'msg_csr_success': 'CSR generated',
    }
};

// Export for module systems or global window object
if (typeof module !== 'undefined' && module.exports) {
    module.exports = SKFClient;
} else {
    window.SKFClient = SKFClient;
}
