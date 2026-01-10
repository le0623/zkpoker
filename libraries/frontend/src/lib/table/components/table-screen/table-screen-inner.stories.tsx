import type { Meta, StoryObj } from "@storybook/react";

import { RawUserContextProvider } from '@lib/user';
import { ActionLog, Card, SidePot } from '@declarations/table_canister/table_canister.did';
import { AllMockUsers, mockTable, mockTableContext, mockUserArray } from '@lib/utils/mock';
import { FloatToTokenAmount } from '@lib/utils/token-amount-conversion';

import { ProvideTable } from '../../context/table.context';
import { TableScreenInnerComponent } from './table-screen.component';

const _meta = { decimals: 8, thousands: 10 ** 8, transactionFee: 10_000n };

type TableStories = {
  userCount: number;
  seats: number;
  foldCount: number;
  isJoined: boolean;
  community_cards: Card[];
  incomingShowCardsRequest?: number;
  winnings?: {
    user_index: number;
    amount: bigint;
  }[];

  sidepots: number[];
  pot: number;
};

const meta: Meta<TableStories> = {
  title: "Table/Screen/Container",
  args: {
    seats: 6,
    userCount: 2,
    foldCount: 0,
    sidepots: [],
    pot: 0,
    community_cards: [],
    isJoined: true,
  },
  parameters: {
    viewport: {
      viewports: {
        smallLaptop: {
          name: "Small Laptop",
          styles: {
            width: "1024px",
            height: "600px",
          },
        },
        reasonableLaptop: {
          name: "Reasonable Laptop",
          styles: {
            width: "1280px",
            height: "800px",
          },
        },
        largeLaptop: {
          name: "Large Laptop",
          styles: {
            width: "1440px",
            height: "900px",
          },
        },
      },
    },
  },
  component: TableScreenInnerComponent,
  render: ({
    userCount,
    seats,
    isJoined,
    community_cards,
    pot,
    sidepots,
    foldCount,
    incomingShowCardsRequest,
    winnings,
  }) => {
    const users = mockUserArray(
      userCount,
      !incomingShowCardsRequest
        ? undefined
        : (data, i) => ({
          ...data,
          cumulative: {
            ...data.cumulative,
            show_card_requests: [
              {
                user_principal: AllMockUsers[1].user.principal_id,
                show_cards: false,
                amount: FloatToTokenAmount(incomingShowCardsRequest, _meta),
              },
            ],
            player_action: foldCount < i ? { Folded: null } : { None: null },
          },
        }),
    );

    const _actionLogs: Omit<ActionLog, "timestamp">[] = [
      ...(winnings
        ? [
          {
            action_type: { Stage: { stage: { Showdown: null } } },
            timestamp: 0n,
            // user_principal: users[0].user.principal_id,
            user_principal: [],
          } as ActionLog,

          ...winnings.map(
            ({ user_index, amount }) =>
              ({
                action_type: { Win: { amount } },
                timestamp: 0n,
                user_principal: [users[user_index].user.principal_id],
              }) as ActionLog,
          ),
        ]
        : []),
      {
        action_type: { Stage: { stage: { Flop: null } } },
        timestamp: 0n,
        user_principal: [],
      },
      ...users
        .filter(({ cumulative }) => "Folded" in cumulative.data!.player_action)
        .map(
          ({ user }): ActionLog => ({
            action_type: { Fold: null },
            timestamp: 0n,
            user_principal: [user.principal_id],
          }),
        ),
      {
        action_type: { Stage: { stage: { Opening: null } } },
        timestamp: 0n,
        user_principal: [],
      },
      ...users.map(
        ({ user },): ActionLog => ({
          action_type: { Raise: { amount: FloatToTokenAmount(5, _meta) } },
          timestamp: 0n,
          user_principal: [user.principal_id],
        }),
      ),
    ];

    return (
      <RawUserContextProvider
        value={{
          isLoading: false,
          ...users[0],
          show: () => { },
          showProfile: () => { },
          showSignup: () => { },
        }}
      >
        <ProvideTable
          value={mockTableContext({
            users: users.map(({ cumulative }) => cumulative),
            currentBet: 0n,
            table: mockTable({
              community_cards,
              users: {
                users: users.map(({ user }) => [user.principal_id, user]),
              },
              seats: users.map(({ user }) => ({ Occupied: user.principal_id })),
              user_table_data: users.map(({ cumulative }) => [
                cumulative.canister_id!,
                cumulative.data!,
              ]),
              pot: FloatToTokenAmount(pot, _meta),
              side_pots: sidepots.map(
                (amount, i): SidePot => ({
                  pot: FloatToTokenAmount(amount, _meta),
                  confirmed_pot: FloatToTokenAmount(amount, _meta),
                  highest_bet: FloatToTokenAmount(0, _meta),
                  user_principals: users
                    .filter((_, _i) => _i > i)
                    .map(({ user }) => user.principal_id),
                }),
              ),
              config: {
                seats,
              },
              action_logs: _actionLogs.map(
                (log, i): ActionLog => ({ ...log, timestamp: BigInt(i) }),
              ),
            }),
            isJoined,
            getSeat: (index: number) => users[index]?.cumulative,
            user: isJoined ? users[0].cumulative : undefined,
          })}
        >
          <TableScreenInnerComponent openRngDashboard={() => {}} />
        </ProvideTable>
      </RawUserContextProvider>
    );
  },
};

export default meta;

type Story = StoryObj<TableStories>;

export const UI: Story = { args: {} };
export const UIIncomingShowCardsRequest: Story = {
  args: { incomingShowCardsRequest: 1234 },
};

// Each seat count has a different layout
export const UI2Seats: Story = { args: { isJoined: true, seats: 2 } };
export const UI3Seats: Story = { args: { isJoined: false, seats: 3 } };
export const UI4Seats: Story = { args: { isJoined: false, seats: 4 } };
export const UI5Seats: Story = { args: { isJoined: false, seats: 5 } };
export const UI6Seats: Story = { args: { isJoined: false, seats: 6 } };
export const UI7Seats: Story = { args: { isJoined: false, seats: 7 } };
export const UI8Seats: Story = { args: { isJoined: false, seats: 8 } };
export const UI9Seats: Story = { args: { isJoined: false, seats: 9 } };
export const UI10Seats: Story = { args: { isJoined: false, seats: 10 } };
export const UI10SeatsWithFolds: Story = {
  args: { isJoined: false, seats: 10, userCount: 10, foldCount: 10 },
};
export const UI10SeatsSidepotsWin: Story = {
  args: {
    isJoined: true,
    userCount: 10,
    seats: 10,
    winnings: [
      { user_index: 0, amount: FloatToTokenAmount(100, _meta) },
      { user_index: 0, amount: FloatToTokenAmount(2, _meta) },
      { user_index: 2, amount: FloatToTokenAmount(100, _meta) },
      { user_index: 5, amount: FloatToTokenAmount(0, _meta) },
      { user_index: 5, amount: FloatToTokenAmount(100, _meta) },
    ],
  },
};

const fullTableArgs = { isJoined: false, seats: 10, userCount: 10 };

export const UI10SeatsFilled: Story = { args: fullTableArgs };
export const UI10SeatsFilledSmallScreen: Story = {
  args: fullTableArgs,
  parameters: { viewport: { defaultViewport: "smallLaptop" } },
};
export const UI10SeatsFilledReasonableScreen: Story = {
  args: fullTableArgs,
  parameters: { viewport: { defaultViewport: "reasonableLaptop" } },
};
export const UI10SeatsFilledLargeScreen: Story = {
  args: fullTableArgs,
  parameters: { viewport: { defaultViewport: "largeLaptop" } },
};

export const UI10SeatsFilledSidepots: Story = {
  args: {
    ...fullTableArgs,
    pot: 100,
    sidepots: [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000],
  },
  parameters: { viewport: { defaultViewport: "smallLaptop" } },
};
