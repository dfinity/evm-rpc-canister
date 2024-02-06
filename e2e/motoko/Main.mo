import EvmRpcProductionFiduciary "canister:evm_rpc";
import EvmRpcStaging13Node "canister:evm_rpc_staging_13_node";
import EvmRpcStagingFidicuary "canister:evm_rpc_staging_fiduciary";

import Blob "mo:base/Blob";
import Buffer "mo:base/Buffer";
import Debug "mo:base/Debug";
import Cycles "mo:base/ExperimentalCycles";
import Principal "mo:base/Principal";
import Text "mo:base/Text";
import Evm "mo:evm";

shared ({ caller = installer }) actor class Main() {

    type TestCategory = { #staging; #production };

    // (`subnet name`, `nodes in subnet`, `expected cycles for JSON-RPC call`)
    type SubnetTarget = (Text, Nat32, Nat);
    let defaultSubnet: SubnetTarget = ("13-node", 13, 99_330_400);
    let fiduciarySubnet: SubnetTarget = ("fiduciary", 28, 239_142_400);

    let testTargets = [
        // (`canister module`, `canister type`, `subnet`)
        (EvmRpcStaging13Node, #staging, defaultSubnet),
        (EvmRpcStagingFidicuary, #staging, fiduciarySubnet),
        (EvmRpcProductionFiduciary, #production, fiduciarySubnet),
    ];

    // (`RPC service`, `method`)
    let ignoredTests = [
        (#EthMainnet(#BlockPi), "eth_sendRawTransaction"), // "Private transaction replacement (same nonce) with gas price change lower than 10% is not allowed within 30 sec from the previous transaction."
    ];

    func runTests(caller : Principal, category : TestCategory) : async () {
        assert caller == installer;

        let errors = Buffer.Buffer<Text>(0);
        var relevantTestCount = 0;
        label targets for ((canister, testCategory, (subnetName, nodesInSubnet, expectedCycles)) in testTargets.vals()) {
            if (testCategory != category) {
                continue targets;
            };
            relevantTestCount += 1;

            let canisterType = debug_show category # " " # subnetName;
            Debug.print("Testing " # canisterType # " canister...");

            func addError(error : Text) {
                let message = "[" # canisterType # "] " # error;
                Debug.print(message);
                errors.add(message);
            };

            let mainnet = Evm.Rpc(
                #Canister canister,
                #Service {
                    hostname = "cloudflare-eth.com";
                    network = ? #EthMainnet;
                },
            );

            let source = #Service {
                hostname = "cloudflare-eth.com";
                chainId = ?(1 : Nat64); // Ethereum mainnet
            };
            let json = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":null,\"id\":1}";
            let maxResponseBytes : Nat64 = 1000;

            // Nodes in subnet
            let actualNodesInSubnet = await canister.getNodesInSubnet();
            if (actualNodesInSubnet != nodesInSubnet) {
                addError("Unexpected number of nodes in subnet (received " # debug_show actualNodesInSubnet # ", expected " # debug_show nodesInSubnet # ")");
            };

            // `requestCost()`
            let cyclesResult = await canister.requestCost(source, json, maxResponseBytes);
            let cycles = switch cyclesResult {
                case (#Ok cycles) { cycles };
                case (#Err err) {
                    Debug.trap("Unexpected error for `requestCost`: " # debug_show err);
                };
            };

            if (cycles != expectedCycles) {
                addError("Unexpected number of cycles: " # debug_show cycles # " (expected " # debug_show expectedCycles # ")");
            };

            // `request()` without cycles
            let resultWithoutCycles = await canister.request(source, json, maxResponseBytes);
            assert switch resultWithoutCycles {
                case (#Err(#ProviderError(#TooFewCycles { expected }))) expected == cycles;
                case _ false;
            };

            // `request()` with cycles
            let result = await mainnet.request("eth_gasPrice", #Array([]), 1000);
            label validate {
                switch result {
                    case (#ok(#Object fields)) {
                        for ((key, val) in fields.vals()) {
                            switch (key, val) {
                                case ("result", #String val) {
                                    assert Text.startsWith(val, #text "0x");
                                    break validate;
                                };
                                case _ {};
                            };
                        };
                    };
                    case _ {};
                };
                addError(debug_show result);
            };

            // `request()` without sufficient cycles
            let resultWithoutEnoughCycles = await canister.request(source, json, maxResponseBytes);
            Cycles.add(cycles - 1);
            assert switch resultWithoutEnoughCycles {
                case (#Err(#ProviderError(#TooFewCycles { expected }))) expected == cycles;
                case _ false;
            };

            // Candid-RPC methods
            type RpcResult<T> = { #Ok : T; #Err : canister.RpcError };
            type MultiRpcResult<T> = {
                #Consistent : RpcResult<T>;
                #Inconsistent : [(canister.RpcService, RpcResult<T>)];
            };

            func assertOk<T>(method : Text, result : MultiRpcResult<T>) {
                switch result {
                    case (#Consistent(#Ok _)) {};
                    case (#Consistent(#Err err)) {
                        addError("Received consistent error for" # " " # method # ": " # debug_show err);
                    };
                    case (#Inconsistent(results)) {
                        for ((service, result) in results.vals()) {
                            switch result {
                                case (#Ok(_)) {};
                                case (#Err(err)) {
                                    for ((ignoredService, ignoredMethod) in ignoredTests.vals()) {
                                        if (service == ignoredService and method == ignoredMethod) {
                                            Debug.print("Ignoring error from " # canisterType # " " # debug_show ignoredService # " " # ignoredMethod);
                                            return;
                                        };
                                    };
                                    addError("Received error in inconsistent results for " # debug_show service # " " # method # ": " # debug_show err);
                                };
                            };
                        };
                    };
                };
            };

            let candidRpcCycles = 1_000_000_000_000;
            let ethMainnetSource = #EthMainnet(?[#Alchemy, #Ankr, #Cloudflare, #BlockPi, #PublicNode]);

            switch (await canister.eth_getBlockByNumber(ethMainnetSource, null, #Latest)) {
                case (#Consistent(#Err(#ProviderError(#TooFewCycles _)))) {};
                case result {
                    addError("Received unexpected result: " # debug_show result);
                };
            };

            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_getLogs",
                await canister.eth_getLogs(
                    ethMainnetSource,
                    null,
                    {
                        addresses = ["0xB9B002e70AdF0F544Cd0F6b80BF12d4925B0695F"];
                        fromBlock = null;
                        toBlock = null;
                        topics = ?[
                            ["0x4d69d0bd4287b7f66c548f90154dc81bc98f65a1b362775df5ae171a2ccd262b"],
                            [
                                "0x000000000000000000000000352413d00d2963dfc58bc2d6c57caca1e714d428",
                                "0x000000000000000000000000b6bc16189ec3d33041c893b44511c594b1736b8a",
                            ],
                        ];
                    },
                ),
            );
            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_getBlockByNumber",
                await canister.eth_getBlockByNumber(ethMainnetSource, null, #Latest),
            );
            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_getTransactionReceipt",
                await canister.eth_getTransactionReceipt(ethMainnetSource, null, "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f"),
            );
            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_getTransactionCount",
                await canister.eth_getTransactionCount(
                    ethMainnetSource,
                    null,
                    {
                        address = "0x1789F79e95324A47c5Fd6693071188e82E9a3558";
                        block = #Latest;
                    },
                ),
            );
            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_feeHistory",
                await canister.eth_feeHistory(
                    ethMainnetSource,
                    null,
                    {
                        blockCount = 3;
                        newestBlock = #Latest;
                        rewardPercentiles = null;
                    },
                ),
            );
            Cycles.add(candidRpcCycles);
            assertOk(
                "eth_sendRawTransaction",
                await canister.eth_sendRawTransaction(
                    ethMainnetSource,
                    null,
                    "0xf86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83",
                ),
            );
        };

        if (relevantTestCount == 0) {
            Debug.trap("No tests found for category: " # debug_show category);
        };

        if (errors.size() > 0) {
            var message = "Errors:";
            for (error in errors.vals()) {
                message #= "\n* " # error;
            };
            Debug.trap(message);
        };
    };

    public shared ({ caller }) func test() : async () {
        await runTests(caller, #staging);
    };

    public shared ({ caller }) func testProduction() : async () {
        await runTests(caller, #production);
    };
};
