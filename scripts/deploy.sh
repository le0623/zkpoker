#!/bin/bash

# Function to display help message
show_help() {
  echo "Usage: $0 <local|mainnet|update> [canister_names...] [--help]"
  echo ""
  echo "Options:"
  echo "  local          Deploy canisters to the local network."
  echo "  mainnet        Deploy canisters to the mainnet."
  echo "  update         Update the .did files. Can specify 'all' or a list of canister names."
  echo "  --help         Display this help message."
  exit 0
}

compile_and_extract() {
  local canister=$1

  echo "Compiling the Canister Wasm module for $canister..."
  cargo build --release --target wasm32-unknown-unknown --package "$canister"

  if [ $? -ne 0 ]; then
    echo "Failed to compile $canister."
    exit 1
  fi

  local wasm_path="target/wasm32-unknown-unknown/release/${canister}.wasm"
  local did_path="./src/${canister}/${canister}.did"

  if [ ! -f "$wasm_path" ]; then
    echo "Error: Wasm file not found at $wasm_path"
    read -p "Would you like to skip this canister and continue? (yes/no): " skip_confirm
    if [ "$skip_confirm" == "yes" ]; then
      return
    else
      exit 1
    fi
  fi

  echo "Extracting candid from the Wasm module and saving to $did_path..."
  if ! command -v candid-extractor &> /dev/null; then
    echo "Error: candid-extractor is not installed."
    read -p "Would you like to install it now? (yes/skip/abort): " action
    case $action in
      yes)
        cargo install candid-extractor
        if [ $? -ne 0 ]; then
          echo "Failed to install candid-extractor. Please install it manually."
          exit 1
        fi
        ;;
      skip)
        echo "Skipping candid extraction for $canister."
        return
        ;;
      *)
        echo "Aborting."
        exit 1
        ;;
    esac
  fi

  candid-extractor "$wasm_path" > "$did_path"
  if [ $? -ne 0 ]; then
    echo "Failed to extract candid for $canister."
    read -p "Would you like to skip this canister and continue? (yes/no): " skip_confirm
    if [ "$skip_confirm" == "yes" ]; then
      return
    else
      exit 1
    fi
  fi

  echo "Successfully extracted candid for $canister."
}

# Function to deploy canisters based on the environment
deploy_canisters() {
  local environment=$1

  ./scripts/download-canister-files.sh
  dfx generate

  if [ "$environment" == "local" ]; then
    echo "Deploying canisters to local network..."
    dfx deps pull
    dfx deps deploy
    dfx deploy
  elif [ "$environment" == "mainnet" ]; then
    echo "Current dfx identity:"
    dfx identity whoami
    echo ""
    read -p "Are you sure you want to deploy to the mainnet? (yes/no): " confirm
    if [ "$confirm" == "yes" ]; then
      echo "Deploying canisters to mainnet..."
      dfx deploy --network ic
    else
      echo "Mainnet deployment aborted."
      exit 1
    fi
  else
    echo "Invalid environment specified. Use 'local' or 'mainnet'."
    exit 1
  fi
}

# Function to update .did files for specific canisters
update_did_files() {
  canisters=("$@")

  cargo build --release --target wasm32-unknown-unknown --package table_canister
  cargo build --release --target wasm32-unknown-unknown --package tournament_canister
  cargo build --release --target wasm32-unknown-unknown --package clans_canister
  cargo build --release --target wasm32-unknown-unknown --package users_canister
  
  for canister in $canisters; do
    compile_and_extract "$canister"
  done
}

# Main script execution
if [ -z "$1" ]; then
  show_help
fi

if [ "$1" == "--help" ]; then
  show_help
fi

operation=$1
shift

# Find all Rust packages in the workspace
rust_packages=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.manifest_path | contains("/src/")) | .name')

case $operation in
  local | mainnet)
    for canister in $rust_packages; do
      compile_and_extract "$canister"
    done
    deploy_canisters "$operation"
    ;;
  update)
    if [ "$1" == "all" ]; then
      echo "Updating all .did files..."
      update_did_files "${rust_packages[@]}"
    else
      update_did_files "$@"
    fi
    ;;
  *)
    show_help
    ;;
esac

echo "Script execution completed."
