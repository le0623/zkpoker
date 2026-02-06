import { memo, useEffect, useMemo, useState } from 'react';

import type { _SERVICE } from '@declarations/table_canister/table_canister.did';
import { ErrorComponent, LoadingAnimationComponent, Modal } from '@zk-game-dao/ui';

import { useTable } from '../../context/table.context';
import type { RngMetadata } from '../../types/rng.types';
import { RngDeckPanelComponent } from './rng-deck-panel.component';
import { RngSourcePanelComponent } from './rng-source-panel.component';
import { RngTransparencyGuideComponent } from './rng-transparency-guide.component';
import { RngVerificationPanelComponent } from './rng-verification-panel.component';

interface RngDashboardModalProps {
  show: boolean;
  onClose: () => void;
  roundId?: bigint;
  tableActor: _SERVICE;
}

type VerificationStatus = "idle" | "pending" | "verified" | "failed";

export const RngDashboardModalComponent = memo<RngDashboardModalProps>(({
  show,
  onClose,
  roundId,
  tableActor,
}) => {
  const { table } = useTable();
  const [rngData, setRngData] = useState<RngMetadata | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [verificationStatus, setVerificationStatus] =
    useState<VerificationStatus>("idle");

  // Calculate if game has ended (for security checks)
  const gameEnded = useMemo(() => {
    if (!rngData) return false;
    
    // ✅ PRIMARY CHECK: Use table state (most reliable indicator)
    // sorted_users is set when the game ends and winners are determined
    const firstSortedUsers = table?.sorted_users?.[0];
    if (firstSortedUsers !== undefined && firstSortedUsers.length > 0) {
      return true;
    }
    
    // ✅ SECONDARY CHECK: Deck should be populated after fix
    // But don't rely solely on this - it's a backup check
    // Note: shuffled_deck is now stored at creation but hidden during gameplay
    if (rngData.shuffled_deck.length === 52) {
      return true;
    }
    
    return false;
  }, [rngData, table]);

  useEffect(() => {
    if (show) {
      loadRngData();
    }
  }, [show, roundId]);

  async function loadRngData() {
    setLoading(true);
    setError(null);
    try {
      const result = roundId
        ? await tableActor.get_rng_metadata(roundId)
        : await tableActor.get_current_rng_metadata();
      if ("Ok" in result) {
        setRngData(result.Ok);
        setError(null);
      } else if ("Err" in result) {
        setError("Failed to load RNG data");
        setRngData(null);
      }
    } catch (err) {
      console.error("Error loading RNG data:", err);
      setError(err instanceof Error ? err.message : "Unknown error occurred");
      setRngData(null);
    } finally {
      setLoading(false);
    }
  }

  async function handleVerify() {
    if (!rngData) return;

    setVerificationStatus("pending");
    try {
      const result = await tableActor.verify_shuffle(rngData.round_id);

      if ("Ok" in result && result.Ok) {
        setVerificationStatus("verified");
      } else {
        setVerificationStatus("failed");
      }
    } catch (err) {
      console.error("Verification error:", err);
      setVerificationStatus("failed");
    }
  }

  // Close on ESC key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape" && show) {
        onClose();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [show, onClose]);

  if (!show) return null;

  return (
    <Modal contentClassName='min-w-[1000px]' open={show} title="RNG Transparency" onClose={onClose}>
      {loading && (
        <LoadingAnimationComponent>Loading RNG data</LoadingAnimationComponent>
      )}

      {error && (
        <ErrorComponent error={error} />
      )}

      {!loading && !error && rngData && (
        <>
          <RngDeckPanelComponent rngData={rngData} tableActor={tableActor} roundId={roundId} />
          <RngTransparencyGuideComponent />
          <RngSourcePanelComponent rngData={rngData} />
          <RngVerificationPanelComponent
            verificationStatus={verificationStatus}
            onVerify={handleVerify}
            rngData={rngData}
            gameEnded={gameEnded}
          />
        </>
      )}

      {!loading && !error && !rngData && (
        <ErrorComponent error="No RNG data available for this round" />
      )}
    </Modal>
  );
});
RngDashboardModalComponent.displayName = "RngDashboardModalComponent";
