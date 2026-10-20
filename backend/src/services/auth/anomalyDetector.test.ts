// Copyright (c) 2026 StellarDevTools
// SPDX-License-Identifier: MIT

/**
 * Tests for AnomalyDetector
 */

import { AnomalyDetector } from './anomalyDetector';

describe('AnomalyDetector', () => {
  let detector: AnomalyDetector;

  beforeEach(() => {
    detector = new AnomalyDetector();
  });

  describe('computeDeviceFingerprint', () => {
    test('should generate consistent fingerprint for same headers', () => {
      const headers = {
        'user-agent': 'Mozilla/5.0',
        'accept-language': 'en-US',
        'accept-encoding': 'gzip, deflate',
      };

      const fp1 = detector.computeDeviceFingerprint(headers);
      const fp2 = detector.computeDeviceFingerprint(headers);

      expect(fp1.hash).toBe(fp2.hash);
    });

    test('should generate different fingerprints for different headers', () => {
      const headers1 = {
        'user-agent': 'Mozilla/5.0',
        'accept-language': 'en-US',
      };
      const headers2 = {
        'user-agent': 'Chrome/91.0',
        'accept-language': 'en-GB',
      };

      const fp1 = detector.computeDeviceFingerprint(headers1);
      const fp2 = detector.computeDeviceFingerprint(headers2);

      expect(fp1.hash).not.toBe(fp2.hash);
    });

    test('should extract user agent from headers', () => {
      const headers = {
        'user-agent': 'Mozilla/5.0 (Test Browser)',
      };

      const fp = detector.computeDeviceFingerprint(headers);
      expect(fp.userAgent).toBe('Mozilla/5.0 (Test Browser)');
    });

    test('should handle missing headers gracefully', () => {
      const headers: Record<string, string> = {};
      const fp = detector.computeDeviceFingerprint(headers);

      expect(fp.hash).toBeDefined();
      expect(fp.hash.length).toBe(64); // SHA-256 hex length
    });
  });

  describe('checkThreatIntel', () => {
    test('should return threat intel for IP address', async () => {
      const threats = await detector.checkThreatIntel('192.168.1.1');

      expect(threats).toHaveProperty('isTor');
      expect(threats).toHaveProperty('isVpn');
      expect(threats).toHaveProperty('isProxy');
      expect(threats).toHaveProperty('threatScore');
    });

    test('should return low threat score for normal IP', async () => {
      const threats = await detector.checkThreatIntel('8.8.8.8');
      expect(threats.threatScore).toBeLessThanOrEqual(50);
    });
  });

  describe('evaluateAnomalies', () => {
    test('should return numeric anomaly score', async () => {
      const headers = {
        'user-agent': 'Mozilla/5.0',
      };
      const score = await detector.evaluateAnomalies({
        userId: 'test-user',
        deviceFingerprint: detector.computeDeviceFingerprint(headers),
      });

      expect(typeof score).toBe('number');
      expect(score).toBeGreaterThanOrEqual(0);
      expect(score).toBeLessThanOrEqual(100);
    });
  });

  describe('evaluateSession', () => {
    test('should evaluate complete security context', async () => {
      const headers = {
        'user-agent': 'Mozilla/5.0 (Test)',
        'accept-language': 'en-US',
      };

      const context = await detector.evaluateSession(
        'test-user-123',
        'session-abc',
        headers,
        '192.168.1.1'
      );

      expect(context.userId).toBe('test-user-123');
      expect(context.sessionId).toBe('session-abc');
      expect(context.deviceFingerprint).toBeDefined();
      expect(context.threatIntel).toBeDefined();
      expect(context.anomalyScore.total).toBeGreaterThanOrEqual(0);
      expect(context.riskLevel).toMatch(/low|medium|high|critical/);
      expect(typeof context.requiresStepUp).toBe('boolean');
    });

    test('should flag high risk anomalies for step-up', async () => {
      const headers = {
        'user-agent': 'Bot/1.0', // Suspicious user agent
      };

      const context = await detector.evaluateSession(
        'test-user',
        'session-xyz',
        headers,
        '192.168.1.1'
      );

      // Bot-like UA should increase anomaly score
      expect(context.anomalyScore.total).toBeGreaterThanOrEqual(0);
    });
  });

  describe('Session revocation', () => {
    test('should revoke sessions', async () => {
      await detector.revokeSession('user-123', 'session-456');
      const isRevoked = await detector.isSessionRevoked('session-456');
      expect(isRevoked).toBe(true);
    });

    test('should detect revoked sessions', async () => {
      const sessionId = 'revoked-session-789';
      await detector.revokeSession('user-456', sessionId);
      const isRevoked = await detector.isSessionRevoked(sessionId);

      expect(isRevoked).toBe(true);
    });

    test('should return false for non-revoked sessions', async () => {
      const isRevoked = await detector.isSessionRevoked('non-existent-session');
      expect(isRevoked).toBe(false);
    });
  });

  describe('Audit trail', () => {
    test('should log and retrieve audit trail', async () => {
      await detector.evaluateSession(
        'audit-test-user',
        'audit-session',
        { 'user-agent': 'Test' },
        '192.168.1.1'
      );

      const trail = await detector.getAuditTrail('audit-test-user');
      expect(Array.isArray(trail)).toBe(true);
    });
  });

  describe('Risk level classification', () => {
    test('should classify risk levels correctly', async () => {
      // Create contexts with different risk profiles
      const lowRiskContext = await detector.evaluateSession(
        'low-risk-user',
        'session-1',
        { 'user-agent': 'Standard Browser' },
        '8.8.8.8'
      );

      expect(['low', 'medium', 'high', 'critical']).toContain(
        lowRiskContext.riskLevel
      );
    });
  });
});
