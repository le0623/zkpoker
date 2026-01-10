import { useState, useEffect, useCallback } from 'react';
import type { RngMetadata } from '../../types/rng.types';
import type { _SERVICE } from '@declarations/table_canister/table_canister.did';
import type { PublicTable } from '@declarations/table_index/table_index.did';
import { useTable } from '../../context/table.context';
import { RngSourcePanel } from './rng-source-panel.component';
import { RngDeckPanel } from './rng-deck-panel.component';
import { RngVerificationPanel } from './rng-verification-panel.component';
import { RngTransparencyGuide } from './rng-transparency-guide.component';
import { RngManualVerify } from './rng-manual-verify.component';
import './rng-dashboard.styles.css';

interface RngDashboardProps {
  isOpen: boolean;
  onClose: () => void;
  roundId?: bigint;
  tableActor: _SERVICE;
}

type VerificationStatus = 'idle' | 'pending' | 'verified' | 'failed';

export function RngDashboard({
  isOpen,
  onClose,
  roundId,
  tableActor,
}: RngDashboardProps) {
  const [rngData, setRngData] = useState<RngMetadata | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [verificationStatus, setVerificationStatus] = useState<VerificationStatus>('idle');

  useEffect(() => {
    if (isOpen) {
      loadRngData();
    }
  }, [isOpen, roundId]);

  async function loadRngData() {
    setLoading(true);
    setError(null);

    try {
      const result = roundId
        ? await tableActor.get_rng_metadata(roundId)
        : await tableActor.get_current_rng_metadata();

      if ('Ok' in result) {
        setRngData(result.Ok);
        setError(null);
      } else if ('Err' in result) {
        setError('Failed to load RNG data');
        setRngData(null);
      }
    } catch (err) {
      console.error('Error loading RNG data:', err);
      setError(err instanceof Error ? err.message : 'Unknown error occurred');
      setRngData(null);
    } finally {
      setLoading(false);
    }
  }

  async function handleVerify() {
    if (!rngData) return;

    setVerificationStatus('pending');
    try {
      const result = await tableActor.verify_shuffle(rngData.round_id);

      if ('Ok' in result && result.Ok) {
        setVerificationStatus('verified');
      } else {
        setVerificationStatus('failed');
      }
    } catch (err) {
      console.error('Verification error:', err);
      setVerificationStatus('failed');
    }
  }

  // Close on ESC key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="rng-dashboard-overlay" onClick={onClose}>
      <div
        className="rng-dashboard-container"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="rng-dashboard-header">
          <h2>🎲 RNG Transparency</h2>
          <button
            onClick={onClose}
            className="close-button"
            aria-label="Close RNG Dashboard"
          >
            ✕
          </button>
        </div>

        {/* Content */}
        <div className="rng-dashboard-content">
          {loading && (
            <div className="loading-state">
              <div className="spinner" />
              <p>Loading RNG data...</p>
            </div>
          )}

          {error && (
            <div className="error-state">
              <p>❌ {error}</p>
              <button onClick={loadRngData} className="retry-button">
                Retry
              </button>
            </div>
          )}

          {!loading && !error && rngData && (
            <>
              <RngTransparencyGuide />
              <RngSourcePanel rngData={rngData} />
              <RngDeckPanel rngData={rngData} tableActor={tableActor} />
              <RngManualVerify rngData={rngData} />
              <RngVerificationPanel
                verificationStatus={verificationStatus}
                onVerify={handleVerify}
                rngData={rngData}
              />
            </>
          )}

          {!loading && !error && !rngData && (
            <div className="empty-state">
              <p>No RNG data available for this round</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

