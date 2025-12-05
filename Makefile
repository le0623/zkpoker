build:
	./scripts/build-canisters.sh

test:
	./scripts/run-tests.sh


update-all:
	./scripts/deploy.sh update all

upgrade-user-canister:
	./scripts/upgrade_user_canisters.sh

upgrade-table-canister:
	./scripts/upgrade_table_canisters.sh

#local
run:
	dfx start --clean --background

stop:
	dfx stop && dfx killall

local-deploy:
	./scripts/deploy.sh local

#mainnet
mainnet-deploy:
	./scripts/deploy.sh mainnet

top-up-canister:
	./scripts/top_up_canisters.sh
