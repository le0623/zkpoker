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
