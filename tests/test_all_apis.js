const SKFClient = require('../api/skf_api.js');
const { WebSocket } = require('ws');
const fs = require('fs');
const { execSync } = require('child_process');

// Polyfill WebSocket for Node.js
global.WebSocket = WebSocket;

const WS_URL = process.env.SKF_WS_URL || 'ws://127.0.0.1:9001';
const PIN = process.env.SKF_PIN || '12345678';
const TIMEOUT_MS = 60000; // 60 seconds max

let passed = 0, failed = 0, skipped = 0;
const results = [];

function check(label, ok, detail) {
    if (ok) {
        passed++;
        results.push({ label, status: '✅', detail });
        console.log(`  ✅ ${label}${detail ? ` (${detail})` : ''}`);
    } else {
        failed++;
        results.push({ label, status: '❌', detail });
        console.log(`  ❌ ${label}${detail ? ` (${detail})` : ''}`);
    }
}

function skip(label, reason) {
    skipped++;
    results.push({ label, status: '⏭️', detail: reason });
    console.log(`  ⏭️ ${label} (Skipped: ${reason})`);
}

function printSummary() {
    console.log('\n========================================');
    console.log('  Consolidated SKF API Test Results / SKF API 全量测试统计结果');
    console.log('========================================');
    for (const r of results) {
        const detail = r.detail ? ` (${r.detail})` : '';
        console.log(`  ${r.status} ${r.label}${detail}`);
    }
    console.log('----------------------------------------');
    console.log(`  Passed / 通过: ${passed}  Failed / 失败: ${failed}  Skipped / 跳过: ${skipped}`);
    console.log('========================================');
}

async function runTests() {
    const skf = new SKFClient(WS_URL);

    console.log('\n[1. Connection & Setup / 连接与初始化]');
    try {
        await skf.connect();
        check('Connect to WebSocket / 连接 WebSocket', true);
    } catch (e) {
        check('Connect to WebSocket / 连接 WebSocket', false, e.message);
        return;
    }

    let provName = 'GM3000';
    let devName = '';
    let devHandle = null;
    let appName = 'GM3000RSA';

    try {
        const providers = await skf.enumProvider();
        check('EnumProvider / 枚举提供商', providers.length > 0, `Found / 发现: ${providers.join(', ')}`);
        if (providers.length > 0) provName = providers[0];

        const devices = await skf.enumDevice(provName);
        check('EnumDevice / 枚举设备', devices.length > 0, `Found / 发现: ${devices.join(', ')}`);
        if (devices.length > 0) devName = devices[0];

        if (!devName) {
            skip('Subsequent tests / 后续测试', 'No device found / 未发现设备');
            return;
        }

        devHandle = await skf.connectDev(devName);
        check('ConnectDev / 连接设备', !!devHandle);

        const rnd = await skf.generateRandom(devHandle, 16);
        check('GenerateRandom / 生成随机数', typeof rnd === 'string' && rnd.length > 0);

        const apps = await skf.enumApplication(provName, devName);
        check('EnumApplication / 枚举应用', apps.length > 0, `Found / 发现: ${apps.join(', ')}`);
        if (apps.length > 0) appName = apps[0];

        const containers = await skf.enumContainer(provName, devName, appName);
        check('EnumContainer / 枚举容器', Array.isArray(containers));

        console.log('\n[1a. Language Support Verification]');
        const certKeyForLang = `${provName}/${devName}/${appName}`;
        const WRONG_PIN = "88888888"; // Assuming this is wrong

        // Test English
        await skf.setLanguage('EN');
        check('SetLanguage (EN)', true);
        try {
            await skf.checkPIN(certKeyForLang, WRONG_PIN);
            check('CheckPIN with WRONG PIN should fail', false);
        } catch (e) {
            check('English Error Message returned', e.message.includes('VerifyPIN') || !/[\u4e00-\u9fa5]/.test(e.message), e.message);
        }

        // Test Chinese
        await skf.setLanguage('CN');
        check('SetLanguage (CN)', true);
        try {
            await skf.checkPIN(certKeyForLang, WRONG_PIN);
            check('CheckPIN with WRONG PIN should fail', false);
        } catch (e) {
            check('Chinese Error Message returned', /[\u4e00-\u9fa5]/.test(e.message) || e.message.includes('失败'), e.message);
        }

        // Restore EN
        await skf.setLanguage('EN');

    } catch (e) {
        check('Setup Phase', false, e.message);
        return;
    }

    console.log('\n[2. Hash & Digest / 哈希摘要]');
    const testData = Buffer.alloc(32, 0xAA).toString('base64');
    try {
        const digest = await skf.digest(provName, devName, testData, 'SM3');
        check('Digest (SM3) / 生成信息摘要', digest && digest.hex && digest.hex.length === 64);
    } catch (e) {
        check('Digest (SM3) / 生成信息摘要', false, e.message);
    }

    console.log('\n[3. PIN Authentication Refactoring / PIN重构验证]');
    const certKey = `${provName}/${devName}/${appName}`;
    try {
        let authFailOk = false;
        try {
            await skf.createPKCS10(provName, devName, appName, "CN=TestPINRefactor", "SM2", 256);
        } catch (e) {
            authFailOk = true;
        }
        check('CreatePKCS10 blocked without CheckPIN / 未登录状态拦截请求', authFailOk);

        const pinOk = await skf.checkPIN(certKey, PIN);
        check('CheckPIN success / 密码验证及缓存成功', pinOk === true);
    } catch (e) {
        check('PIN Auth Test / 密码验证测试', false, e.message);
    }

    console.log('\n[4. Certificate Issuance (Single) / 单证书签发测试]');
    let singleContainer = '';
    try {
        const csr = await skf.createPKCS10(provName, devName, appName, 'CN=TestSingle,O=Test,C=CN', 'SM2', 256);
        check('CreatePKCS10 (Single) / 生成CSR(单证书)', csr && csr.container && csr.pem);
        singleContainer = csr.container;

        const cert = await skf.issueCertificate(csr.pem, false);
        check('IssueCertificate (Single) / 签发单证书', cert && cert.certificate && !cert.double);

        const importOk = await skf.importCertificate(provName, devName, appName, singleContainer, true, cert.certificate);
        check('ImportCertificate (Sign) / 写入签名证书', importOk === true);
    } catch (e) {
        check('Single Certificate Process / 单证书测试', false, e.message);
    }

    console.log('\n[5. CSR Subject Extraction Verification / 证书请求主题提取验证]');
    let csrSubjectOk = false;
    try {
        const targetSubject = "CN=ExtractedSubject,O=Antigravity,C=CN";
        const csr = await skf.createPKCS10(provName, devName, appName, targetSubject, 'SM2', 256);

        fs.writeFileSync('/tmp/test_test_all_csr.csr', csr.pem);

        const cert = await skf.issueCertificate(csr.pem, false);
        fs.writeFileSync('/tmp/test_test_all_cert.crt', cert.certificate);

        // Use OpenSSL to verify subject
        const subjectOutput = execSync('openssl x509 -in /tmp/test_test_all_cert.crt -noout -subject').toString();
        csrSubjectOk = subjectOutput.includes("ExtractedSubject") && subjectOutput.includes("Antigravity");
        check('CSR Subject Extracted correctly / 证书请求主题提取与验证匹配', csrSubjectOk, subjectOutput.trim());

        // Cleanup this container
        await skf.deleteContainer(provName, devName, appName, csr.container);
    } catch (e) {
        check('CSR Subject Extraction / CSR提取测试', false, e.message);
    }

    console.log('\n[6. Certificate Issuance (Double) / 双证书签发测试]');
    let dualContainer = '';
    try {
        const csr = await skf.createPKCS10(provName, devName, appName, 'CN=TestDual,O=Test,C=CN', 'SM2', 256);
        check('CreatePKCS10 (Double) / 生成CSR(双证书)', csr && csr.container && csr.pem);
        dualContainer = csr.container;

        const cert = await skf.issueCertificate(csr.pem, true);
        check('IssueCertificate (Double) / 签发双证书', cert && cert.double && cert.certificate && cert.certificate2 && cert.encPriKey);

        const importKP = await skf.importKeyPair(provName, devName, appName, dualContainer, cert.alg, cert.encPriKey);
        check('ImportKeyPair / 导入加密私钥对', importKP === true);

        const importSign = await skf.importCertificate(provName, devName, appName, dualContainer, true, cert.certificate);
        check('ImportCertificate (Sign for Double) / 写入签名证书', importSign === true);

        const importEnc = await skf.importCertificate(provName, devName, appName, dualContainer, false, cert.certificate2);
        check('ImportCertificate (Enc for Double) / 写入加密证书', importEnc === true);
    } catch (e) {
        check('Double Certificate Process / 双证书测试', false, e.message);
    }

    console.log('\n[7. Find Certificates & Sign Data / 证书查询与数字签名测试]');
    try {
        const allCerts = await skf.findCertificates('');
        check('FindCertificates / 枚举全局设备内所有证书', Array.isArray(allCerts) && allCerts.length > 0, `Found / 共找到 ${allCerts.length} certs(张)`);

        const signCerts = await skf.findCertificates('Sign');
        if (signCerts.length > 0) {
            const certKeyItem = signCerts[0].key;
            const sig = await skf.signData(certKeyItem, testData);
            check('SignData / 私钥数据签名', typeof sig === 'string' && sig.length > 0);
        } else {
            skip('SignData / 私钥数据签名', 'No signing certificates found / 无签名证书');
        }
    } catch (e) {
        check('Find/Sign Process / 查找与签名测试', false, e.message);
    }

    console.log('\n[8. SM4-CBC Encryption & Decryption / SM4-CBC 加密解密测试]');
    try {
        // 需要一个容器来存储加密密钥对
        if (singleContainer || dualContainer) {
            const testContainer = singleContainer || dualContainer;
            const certKey = `${provName}/${devName}/${appName}/${testContainer}`;

            // 准备测试数据
            const testData = Buffer.from('This is a test message for SM4-CBC encryption!');
            const testDataBase64 = testData.toString('base64');

            // 生成随机 IV (16 bytes for SM4)
            const iv = Buffer.from(await skf.generateRandom(devHandle, 16), 'base64');
            const ivBase64 = iv.toString('base64');

            // 测试加密
            const encryptResult = await skf.encryptData(certKey, testDataBase64, ivBase64, 1); // PKCS5 padding
            check('EncryptData (SM4-CBC) / SM4-CBC 加密', encryptResult && encryptResult.encryptedData);

            // 测试解密
            if (encryptResult && encryptResult.encryptedData) {
                const decryptResult = await skf.decryptData(certKey, encryptResult.encryptedData, ivBase64, 1);
                check('DecryptData (SM4-CBC) / SM4-CBC 解密', decryptResult && decryptResult.data);

                // 验证解密结果
                if (decryptResult && decryptResult.data) {
                    const decryptedData = Buffer.from(decryptResult.data, 'base64');
                    const isValid = decryptedData.equals(testData);
                    check('SM4-CBC Round-trip / SM4-CBC 加解密循环验证', isValid);
                } else {
                    check('SM4-CBC Round-trip / SM4-CBC 加解密循环验证', false, 'Decryption failed');
                }
            } else {
                check('DecryptData (SM4-CBC) / SM4-CBC 解密', false, 'Encryption failed');
                skip('SM4-CBC Round-trip / SM4-CBC 加解密循环验证', 'Encryption failed');
            }
        } else {
            skip('SM4-CBC Tests / SM4-CBC 测试', 'No container available / 无可用容器');
        }
    } catch (e) {
        check('SM4-CBC Tests / SM4-CBC 测试', false, e.message);
    }

    console.log('\n[9. Cleanup / 环境及容器清理]');
    try {
        if (singleContainer) {
            await skf.deleteContainer(provName, devName, appName, singleContainer);
            check('DeleteContainer (Single) / 安全销毁单证书测试容器', true);
        }
        if (dualContainer) {
            await skf.deleteContainer(provName, devName, appName, dualContainer);
            check('DeleteContainer (Double) / 安全销毁双证书测试容器', true);
        }

        await skf.disconnectDev(devHandle);
        check('DisconnectDev / 断开设备', true);
    } catch (e) {
        check('Cleanup Process / 清理测试', false, e.message);
    }

    printSummary();
    skf.disconnect();

    if (failed > 0) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}

// Timeout protection
const timer = setTimeout(() => {
    console.error('Test timeout!');
    process.exit(1);
}, TIMEOUT_MS);

runTests().then(() => clearTimeout(timer));
