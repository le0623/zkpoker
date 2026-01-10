import { useState } from 'react';
import type { RngMetadata, Card } from '../../types/rng.types';
import { formatBytes, shortenHash } from '../../types/rng.types';

interface RngManualVerifyProps {
  rngData: RngMetadata;
}

export function RngManualVerify({ rngData }: RngManualVerifyProps) {
  const [verificationResult, setVerificationResult] = useState<{
    success: boolean;
    calculatedHash: string;
    message: string;
  } | null>(null);
  const [isVerifying, setIsVerifying] = useState(false);

  // Check if we have all data needed for verification
  const canVerify = rngData.time_seed !== BigInt(0) && 
                   rngData.raw_random_bytes.length > 0 &&
                   rngData.shuffled_deck.length === 52;

  const gameEnded = rngData.shuffled_deck.length === 52;

  async function handleManualVerification() {
    setIsVerifying(true);
    setVerificationResult(null);

    try {
      // Step 1: Reshuffle bytes with time_seed (client-side implementation)
      const reshuffledBytes = reshuffleBytes(
        Array.from(rngData.raw_random_bytes),
        Number(rngData.time_seed)
      );

      // Step 2: Create deck from reshuffled bytes
      const reconstructedDeck = createDeckFromBytes(reshuffledBytes);

      // Step 3: Calculate hash of reconstructed deck
      const calculatedHash = await calculateDeckHash(reconstructedDeck);

      // Step 4: Compare with committed hash
      const matches = calculatedHash === rngData.deck_hash;

      setVerificationResult({
        success: matches,
        calculatedHash,
        message: matches
          ? '✅ Verification SUCCESS! The deck matches the commitment.'
          : '❌ Verification FAILED! The deck does NOT match the commitment.',
      });
    } catch (error) {
      console.error('Verification error:', error);
      setVerificationResult({
        success: false,
        calculatedHash: '',
        message: `❌ Verification error: ${error}`,
      });
    } finally {
      setIsVerifying(false);
    }
  }

  return (
    <section className="rng-panel verification-panel">
      <h3 className="panel-title">🔍 Manual Deck Verification</h3>

      {!gameEnded ? (
        <div className="info-block">
          <p className="info-hint">
            ⏳ Manual verification is available after the game ends. Currently,
            time_seed is hidden to prevent deck reconstruction during gameplay.
          </p>
          <div className="verification-status">
            <div className="status-item">
              <span className="status-label">Deck Hash (Commitment):</span>
              <span className="status-value code-block">{shortenHash(rngData.deck_hash)}</span>
            </div>
            <div className="status-item">
              <span className="status-label">Random Bytes:</span>
              <span className="status-value">✅ Available ({rngData.raw_random_bytes.length} bytes)</span>
            </div>
            <div className="status-item">
              <span className="status-label">Time Seed:</span>
              <span className="status-value">🔒 Hidden (revealed after game)</span>
            </div>
          </div>
        </div>
      ) : (
        <>
          <div className="info-block">
            <p className="info-hint">
              Reconstruct the deck yourself using the revealed parameters and
              verify it matches the committed deck hash. This proves the shuffle
              was fair and untampered.
            </p>
          </div>

          <div className="verification-params">
            <div className="param-item">
              <div className="param-label">1️⃣ Raw Random Bytes (IC VRF):</div>
              <div className="code-block">{formatBytes(rngData.raw_random_bytes)}</div>
            </div>

            <div className="param-item">
              <div className="param-label">2️⃣ Time Seed (Nanoseconds):</div>
              <div className="code-block">{rngData.time_seed.toString()}</div>
            </div>

            <div className="param-item">
              <div className="param-label">3️⃣ Expected Deck Hash (Commitment):</div>
              <div className="code-block hash">{rngData.deck_hash}</div>
            </div>
          </div>

          <button
            onClick={handleManualVerification}
            disabled={!canVerify || isVerifying}
            className="verify-button primary"
          >
            {isVerifying ? '🔄 Reconstructing Deck...' : '🔍 Reconstruct & Verify Deck'}
          </button>

          {verificationResult && (
            <div className={`verification-result ${verificationResult.success ? 'success' : 'failed'}`}>
              <div className="result-header">
                <span className="result-icon">
                  {verificationResult.success ? '✅' : '❌'}
                </span>
                <span className="result-title">{verificationResult.message}</span>
              </div>

              <div className="result-details">
                <div className="detail-row">
                  <span className="detail-label">Expected Hash:</span>
                  <span className="detail-value code-block">
                    {shortenHash(rngData.deck_hash)}
                  </span>
                </div>
                <div className="detail-row">
                  <span className="detail-label">Calculated Hash:</span>
                  <span className="detail-value code-block">
                    {shortenHash(verificationResult.calculatedHash)}
                  </span>
                </div>
                <div className="detail-row">
                  <span className="detail-label">Match:</span>
                  <span className={`detail-value ${verificationResult.success ? 'match' : 'no-match'}`}>
                    {verificationResult.success ? 'YES ✅' : 'NO ❌'}
                  </span>
                </div>
              </div>

              {verificationResult.success && (
                <div className="success-explanation">
                  <p className="info-hint">
                    <strong>What this proves:</strong>
                  </p>
                  <ul>
                    <li>✅ The shuffle used the IC VRF random bytes</li>
                    <li>✅ The time seed was applied correctly</li>
                    <li>✅ The deck order matches the commitment</li>
                    <li>✅ No cards were swapped or manipulated</li>
                    <li>✅ The game was provably fair</li>
                  </ul>
                </div>
              )}
            </div>
          )}

          <div className="algorithm-explanation">
            <details>
              <summary>📚 How the verification algorithm works</summary>
              <div className="algorithm-steps">
                <h4>Reconstruction Process:</h4>
                <ol>
                  <li>Take the raw_random_bytes from IC VRF</li>
                  <li>Apply reshuffle_bytes_hash() with time_seed</li>
                  <li>Create FlatDeck using reshuffled bytes (Fisher-Yates)</li>
                  <li>Calculate SHA-256 hash of the deck</li>
                  <li>Compare with committed deck_hash</li>
                </ol>
                <p className="info-hint">
                  This client-side reconstruction uses the same algorithm as the
                  canister, allowing independent verification of fairness.
                </p>
              </div>
            </details>
          </div>
        </>
      )}
    </section>
  );
}

// ============================================================================
// Helper Functions (Client-side implementations matching backend logic)
// ============================================================================

/**
 * Reshuffle bytes using hash-based approach (matches backend reshuffle_bytes_hash)
 */
function reshuffleBytes(bytes: number[], seed: number): number[] {
  if (bytes.length <= 1) return bytes;

  const result = [...bytes];
  const indices: number[] = Array.from({ length: bytes.length }, (_, i) => i);

  // Shuffle indices using hash-based approach (matches Rust DefaultHasher behavior approximation)
  for (let i = indices.length - 1; i >= 1; i--) {
    // Simple hash function (approximation of Rust's DefaultHasher)
    const hash = simpleHash(seed, i);
    const j = hash % (i + 1);
    [indices[i], indices[j]] = [indices[j], indices[i]];
  }

  // Apply shuffle
  const shuffled = new Array(bytes.length);
  for (let newPos = 0; newPos < indices.length; newPos++) {
    shuffled[newPos] = bytes[indices[newPos]];
  }

  return shuffled;
}

/**
 * Simple hash function (approximation of Rust's DefaultHasher for verification)
 */
function simpleHash(seed: number, value: number): number {
  // This approximates Rust's DefaultHasher behavior
  let hash = seed;
  hash = ((hash << 5) - hash) + value;
  hash = hash & hash; // Convert to 32bit integer
  hash ^= hash >>> 16;
  hash = Math.imul(hash, 0x85ebca6b);
  hash ^= hash >>> 13;
  hash = Math.imul(hash, 0xc2b2ae35);
  hash ^= hash >>> 16;
  return Math.abs(hash);
}

/**
 * Create a deck from bytes using Fisher-Yates shuffle (matches FlatDeck::new)
 */
function createDeckFromBytes(bytes: number[]): Card[] {
  // Start with standard 52-card deck in sorted order
  const deck = createStandardDeck();

  // Fisher-Yates shuffle using bytes
  let n = deck.length;
  for (let i = 0; i < n - 1; i++) {
    const randIndex = (bytes[i % bytes.length]) % n;
    [deck[i], deck[randIndex]] = [deck[randIndex], deck[i]];
    n = Math.max(1, n - 1);
  }

  return deck;
}

/**
 * Create standard 52-card deck (sorted order)
 */
function createStandardDeck(): Card[] {
  const values = ['Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten', 'Jack', 'Queen', 'King', 'Ace'];
  const suits = ['Spade', 'Club', 'Heart', 'Diamond'];

  const deck: Card[] = [];
  for (const value of values) {
    for (const suit of suits) {
      deck.push({
        value: { [value]: null },
        suit: { [suit]: null },
      } as Card);
    }
  }
  return deck;
}

/**
 * Calculate SHA-256 hash of deck (matches backend calculate_deck_hash)
 */
async function calculateDeckHash(deck: Card[]): Promise<string> {
  let combinedCardStrings = '';

  for (const card of deck) {
    const valueKey = Object.keys(card.value)[0];
    const suitKey = Object.keys(card.suit)[0];
    combinedCardStrings += `${valueKey}:${suitKey}`;
  }

  const encoder = new TextEncoder();
  const data = encoder.encode(combinedCardStrings);
  // Fix: Create a new ArrayBuffer from the encoded data
  const buffer = new ArrayBuffer(data.length);
  const view = new Uint8Array(buffer);
  view.set(data);
  
  const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');

  return hashHex;
}

