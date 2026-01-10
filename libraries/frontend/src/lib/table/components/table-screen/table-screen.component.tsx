import classNames from 'classnames';
import { isEqual } from 'lodash';
import { lazy, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Helmet } from 'react-helmet';
import { useBlocker } from 'react-router-dom';

import { useRouting } from '@/src/hooks/routing';
import { createActor } from '@declarations/table_canister';
import {
  _SERVICE, ActionLog, PublicTable, User, UserTableData
} from '@declarations/table_canister/table_canister.did';
import { Principal } from '@dfinity/principal';
import { Queries } from '@lib/data';
import { useUser } from '@lib/user';
import { Max } from '@lib/utils/bigint';
import { useQuery } from '@tanstack/react-query';
import { IsSamePrincipal, CurrencyTypeSerializer, useAuth } from '@zk-game-dao/currency';
import {
  ErrorModalComponent, Image, Interactable, IsDev, LoadingAnimationComponent,
  OverrideLayoutConfigComponent, ToggleComponent, UnwrapOptional, useToast
} from '@zk-game-dao/ui';

import { Redirect } from '../../../../components/common/redirect.component';
import { useIsMobile } from '../../../../hooks/screen-size';
import { RefillModal } from '../../../tournament/components/refill-modal.component';
import {
  ProvideRawTournamentContext, useTournament
} from '../../../tournament/context/tournament.context';
import { TableVisualsContext } from '../../context/table-visuals.context';
import {
  ProvideTable, SeatMetaData, TableContextValue, useTable, useTableUrl
} from '../../context/table.context';
import { ActionLogComponent } from '../action-log/action-log.component';
import {
  EnvironmentBackgroundComponent
} from '../environment-background/environment-background.component';
import { HUDComponent } from '../hud/hud.component';
import { TableLeaveModalComponent } from '../table-leave-modal/table-leave-modal.component';
import {
  TableSettingsModalComponent
} from '../table-settings-modal/table-settings-modal.component';
import { TableComponent } from '../table/table.component';
import { TableScreenTournamentModalButton } from './table-screen-tournament-modal.component';
import { ChatComponent } from '../../../../components/common/chat/chat.component';
import { AnimatePresence } from 'framer-motion';
import { useWording } from '../../../../hooks/wording';
import { RngDashboard } from '../rng-dashboard/rng-dashboard.component';

const TableScreenDebugModalButton = lazy(() => import('./table-screen-debug-modal.component'));

export const TableScreenInnerComponent = memo<{ openRngDashboard: (roundId?: bigint) => void }>(({ openRngDashboard }) => {
  const { table, isJoined, user } = useTable();
  const { user: zkpUser } = useUser();
  const [chat, setChat] = useState(false);
  const tournament = useTournament();

  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      isJoined && currentLocation.pathname !== nextLocation.pathname && !tournament,
  );

  // If logged in and not joined, show the hud if there are empty seats
  const hasEmptySeats = useMemo(
    () => table.seats.some(v => "Empty" in v),
    [table?.seats],
  );

  const showHud = useMemo(
    () => {
      if (!table || !zkpUser) return false;
      if (isJoined) return true;
      return hasEmptySeats;
    },
    [hasEmptySeats, isJoined, !!zkpUser, !table],
  );

  return (
    <>
      <TableLeaveModalComponent blocker={blocker} />

      <TableComponent
        className={classNames(
          "z-0 absolute left-8 md:left-16 right-8 top-16 h-md:top-32 transition-[bottom,right]",
          !showHud ? "bottom-16 h-md:bottom-32" : "bottom-32 h-md:bottom-48",
          chat ? 'md:right-[420px]' : 'md:right-16',
        )}
      />

      <AnimatePresence>
        {chat && <ChatComponent />}
      </AnimatePresence>

      {showHud && (
        <div className="fixed bottom-5 w-full z-41">
          <HUDComponent openRngDashboard={openRngDashboard} />
        </div>
      )}

      <div className="absolute bottom-4 right-4 z-42 hidden lg:flex flex-row gap-2 items-center justify-center">
        <ToggleComponent isOn={chat} onChange={setChat}>
          <Image
            type="png"
            width={24}
            height={24}
            src={`/icons/chat-${!chat ? "outline" : "filled"}.png`}
            alt="chat"
          />
        </ToggleComponent>
      </div>

    </>
  );
});
TableScreenInnerComponent.displayName = "TableScreenInnerComponent";

const NavbarItemsComponent = memo<{ openRngDashboard?: (roundId?: bigint) => void }>(({ openRngDashboard }) => {
  const [showSettings, setShowSettings] = useState(false);
  const { user: zkpUser } = useUser();

  if (!zkpUser) return <TableScreenTournamentModalButton />;

  return (
    <>
      <TableScreenTournamentModalButton />
      <Interactable
        className="active:scale-95 flex hover:bg-material-main-1 rounded-[12px]"
        onClick={() => setShowSettings(true)}
      >
        <Image
          type="png"
          width={48}
          height={48}
          src="/icons/settings.png"
          alt="Settings"
        />
      </Interactable>

      <TableSettingsModalComponent
        show={showSettings}
        onClose={() => setShowSettings(false)}
      />
    </>
  );
});
NavbarItemsComponent.displayName = "NavbarItemsComponent";

const TableComponentInner = memo<{
  actor: _SERVICE;
  table: PublicTable;
}>(({ actor, table }) => {
  const { user: zkpUser } = useUser();
  const { addToast } = useToast();

  useEffect(() => {
    Queries.table.invalidate(table);
  }, [table?.seats]);

  const users = useMemo(
    (): SeatMetaData[] =>
      table?.seats
        ? table?.seats.map(
          (status): SeatMetaData => {
            let canister_id: Principal | undefined;
            let data: UserTableData | undefined;
            let user: User | undefined

            if ("Occupied" in status) canister_id = status.Occupied;
            if ("QueuedForNextRound" in status) {
              canister_id = status.QueuedForNextRound[0];
              user = status.QueuedForNextRound[1];
            }
            if ("Reserved" in status) canister_id = status.Reserved.principal;

            if (canister_id) {
              data = table.user_table_data.find(
                ([id]) => canister_id?.compareTo(id) === "eq",
              )?.[1];
              if (!user)
                user = table.users.users.find(
                  ([id]) => canister_id?.compareTo(id) === "eq",
                )?.[1];
            }
            return ({
              status,
              data,
              user,
              canister_id,
            })
          },
        )
        : [],
    [
      table?.seats,
      table?.user_table_data,
      table?.users.users,
    ],
  );

  const user = useMemo((): SeatMetaData | undefined => {
    if (!zkpUser) return;
    return users.find(({ canister_id }) => canister_id?.compareTo(zkpUser.principal_id) === "eq");
  }, [zkpUser?.principal_id, users]);

  const isJoined = useMemo(() => !!user, [user]);

  const currentBet = useMemo(() => {
    if (!table) return 0n;
    return table.user_table_data.reduce(
      (acc, [, u]) => Max(acc, u.current_total_bet),
      0n,
    );
  }, [table.user_table_data]);

  const getSeat = useCallback(
    (seatIndex: number) => users[seatIndex],
    [users],
  );

  const userIndex = useMemo(
    () =>
      table && zkpUser
        ? BigInt(
          table.seats.findIndex(
            (seat) => {
              if ("Occupied" in seat) return seat.Occupied.compareTo(zkpUser?.principal_id) === "eq";
              return false;
            },
          ),
        )
        : undefined,
    [table.seats, zkpUser?.principal_id.toText()],
  );

  /** Notifications, get the new notifications by listening for changes in the table.action_logs and adding new entries in the useEffect */
  const timeOfTableOpen = useMemo(() => new Date(), []);
  const handledActions = useRef<ActionLog[]>([]);
  const isMobile = useIsMobile();

  useEffect(() => {
    if (!table) return;
    const oldActions = [...handledActions.current];
    // Push the sorted users into the newActions as Win type

    // Filter the newActions by the timestamp of the oldActions
    const newActions = table.action_logs.filter(
      (a) => !oldActions.find((b) => a.timestamp === b.timestamp),
    );
    handledActions.current = [...oldActions, ...newActions];

    if (isMobile) {
      const latest_timestamp = newActions.reduce(
        (acc, v) => (v.timestamp > acc ? v.timestamp : acc),
        0n,
      );

      newActions.push(
        ...table.sorted_users.flat().filter(v => v.amount_won > 0n).map((u): ActionLog => ({
          action_type: { Win: { amount: u.amount_won } },
          user_principal: [u.id],
          timestamp: latest_timestamp * 2n,
        }))
      );
    }

    if (!newActions.length) return;

    const relevantActions = newActions
      .filter(
        ({ timestamp }) =>
          new Date(Number(timestamp / 1000n / 1000n)).getTime() >
          timeOfTableOpen.getTime(),
      )
      .filter(({ action_type }) => {
        if ("Bet" in action_type) return false;
        if ("Win" in action_type && !isMobile) return false;
        if ("Call" in action_type) return false;
        if ("Fold" in action_type) return false;
        if ("BigBlind" in action_type) return false;
        if ("Raise" in action_type) return false;
        if ("AllIn" in action_type) return false;
        if ("SmallBlind" in action_type) return false;
        if ("Check" in action_type) return false;
        return true;
      })
      .filter(
        ({ user_principal }) =>
          !user_principal[0] ||
          !zkpUser ||
          user_principal[0].compareTo(zkpUser?.principal_id) !== "eq",
      );

    if (!relevantActions.length) return;

    relevantActions
      .map((action) => <ActionLogComponent {...action} key={`${action.timestamp} + ${action.user_principal[0]?.toText() ?? '-'}`} />)
      .forEach((children) => addToast({ children }));
  }, [table?.action_logs, timeOfTableOpen, zkpUser?.principal_id.toText(), isMobile]);

  const url = useTableUrl(table);

  const isOngoing = useMemo(
    () =>
      !!table &&
      !table.sorted_users[0] &&
      !("Fresh" in table.deal_stage) &&
      !("Opening" in table.deal_stage),
    [table],
  );

  const currencyType = useMemo(
    () => table.config.currency_type,
    [CurrencyTypeSerializer.serialize(table.config.currency_type)],
  );

  // Refresh the user-experience points every time the table goes to opening
  useEffect(() => {
    if (!table || !("Opening" in table.deal_stage)) return;
    Promise.all(table.user_table_data.map(([principal]) => Queries.userExperiencePoints.invalidate(principal))).then();
  }, [table?.deal_stage]);

  const tournament = useTournament();

  const [rngDashboardOpen, setRngDashboardOpen] = useState(false);
  const [selectedRoundId, setSelectedRoundId] = useState<bigint | undefined>();

  const openRngDashboard = useCallback((roundId?: bigint) => {
    setSelectedRoundId(roundId);
    setRngDashboardOpen(true);
  }, []);

  const closeRngDashboard = useCallback(() => {
    setRngDashboardOpen(false);
    setSelectedRoundId(undefined);
  }, []);

  const tableValue = useMemo((): TableContextValue => ({
    currencyType,
    receiver: {
      principal: table.id,
    },
    actor,
    table,
    isJoined,
    // isQueued,
    userIndex,
    getSeat,
    // getQueuedSeat,
    user,
    users,
    currentBet,
    url,
    isOngoing,
  }), [currencyType, actor, table, isJoined, userIndex, getSeat, user, users, currentBet, url, isOngoing, tournament]);

  const canSeeRefill = useMemo(() =>
    !!(
      tournament?.buyInOptions?.addon.enabled &&
      user &&
      UnwrapOptional(table.config.is_paused)
    ),
    [tournament, table, user],
  );

  const wording = useWording()

  return (
    <ProvideTable value={tableValue}>
      <OverrideLayoutConfigComponent
        navbarRightSide={(
          <ProvideTable value={tableValue}>
            {tournament ?
              <ProvideRawTournamentContext value={tournament}><NavbarItemsComponent openRngDashboard={openRngDashboard} /></ProvideRawTournamentContext> :
              <NavbarItemsComponent openRngDashboard={openRngDashboard} />}
          </ProvideTable>
        )}
        isOverlay
      />

      <EnvironmentBackgroundComponent
        color={Number(table.config.environment_color)}
      >
        <TableVisualsContext.Provider
          value={{
            color: Number(table.config.color),
            cardColor: Number(table.config.card_color),
          }}
        >
          <Helmet>
            <title>{wording.product} - {table.config.name}</title>
          </Helmet>

          <div
            className={classNames("relative w-screen h-screen z-0 overflow-hidden")}
          >
            {IsDev && <TableScreenDebugModalButton table={table} user={zkpUser} />}
            <TableScreenInnerComponent openRngDashboard={openRngDashboard} />

            {canSeeRefill && <RefillModal />}

            <RngDashboard
              isOpen={rngDashboardOpen}
              onClose={closeRngDashboard}
              roundId={selectedRoundId}
              tableActor={actor}
            />
          </div>
        </TableVisualsContext.Provider>
      </EnvironmentBackgroundComponent>
    </ProvideTable>
  );
}, isEqual);
TableComponentInner.displayName = "TableComponentInner";

export const TableScreenComponent = memo<{ table_principal: Principal }>(
  ({ table_principal }) => {
    const { authData } = useAuth();

    const actor = useMemo(() => createActor(table_principal, { agent: authData?.agent }), [authData?.agent]);

    const tournament = useTournament();

    const {
      data: table,
      isLoading: isLoadingTable,
      error: tableError,
    } = useQuery({
      queryKey: Queries.table.key({ id: table_principal }),
      queryFn: async () => {
        if (!actor) throw new Error(`No valid id found in url`);
        try {
          const d = await actor.get_table();   
          if ("Err" in d) throw d.Err;
          return d.Ok;
        } catch (e) {
          console.error(e);
          throw e;
        }
      },
      refetchInterval: 2000,
    });

    const { push } = useRouting();

    if (tableError && "TableNotFound" in tableError) throw tableError;

    if (tournament && ('Cancelled' in tournament.data.state || 'Completed' in tournament.data.state))
      return <Redirect to={`/tournaments/${tournament.data.id}`} />;

    if (!table) {
      if (isLoadingTable)
        return (
          <div className="absolute top-0 left-0 h-screen w-screen flex flex-col justify-center items-center">
            <LoadingAnimationComponent>Loading table</LoadingAnimationComponent>
          </div>
        );
      return (
        <ErrorModalComponent onClose={() => push("/")} title="Error">
          {tableError ? "" + JSON.stringify(tableError) : "Unknown error"}
        </ErrorModalComponent>
      );
    }

    if (table.config.table_type[0] && ('Tournament' in table.config.table_type[0]) && !tournament)
      return <Redirect to={`/tournaments/${table.config.table_type[0].Tournament.tournament_id}/tables/${table.id}`} />;

    return <TableComponentInner actor={actor} table={table} />;
  },
  (prevProps, nextProps) =>
    IsSamePrincipal(prevProps.table_principal, nextProps.table_principal)
);
TableScreenComponent.displayName = "TableScreenComponent";
