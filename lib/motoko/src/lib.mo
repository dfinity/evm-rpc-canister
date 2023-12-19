import Blob "mo:base/Blob";
import Debug "mo:base/Debug";
import Cycles "mo:base/ExperimentalCycles";
import Nat64 "mo:base/Nat64";
import Principal "mo:base/Principal";
import Text "mo:base/Text";
import JSON "mo:json.mo";

import EvmRpc "declarations/evm_rpc";

module {
    public type Network = {
        #EthMainnet;
        #EthSepolia;
        #EthGoerli;
        #Network : Nat64;
    };

    public type Source = {
        #Custom : { url : Text; headers : ?[HttpHeader] };
        #Service : { hostname : Text; network : ?Network };
        #Chain : Network;
        #Provider : Nat64;
    };

    type HttpHeader = {
        name : Text;
        value : Text;
    };

    type ActorSource = EvmRpc.JsonRpcSource;

    public type RpcError = EvmRpc.RpcError;
    public type JsonRpcError = EvmRpc.JsonRpcError;
    public type ProviderError = EvmRpc.ProviderError;
    public type HttpOutcallError = EvmRpc.HttpOutcallError;
    public type ValidationError = EvmRpc.ValidationError;

    public type Error = ProviderError or HttpOutcallError or ValidationError or {
        #JsonRpcError : JsonRpcError;
    };

    public type Result<T> = {
        #ok : T;
        #err : Error;
    };

    public type RpcActor = actor {
        request : shared (ActorSource, Text, Nat64) -> async {
            #Ok : Text;
            #Err : RpcError;
        };
    };

    public type Provider = {
        #Canister : RpcActor;
        #Principal : Principal;
    };

    func wrapChainId(network : Network) : Nat64 {
        // Reference: https://chainlist.org/?testnets=true
        switch network {
            case (#EthMainnet) { 1 };
            case (#EthGoerli) { 5 };
            case (#EthSepolia) { 11155111 };
            case (#Network n) { n };
        };
    };

    func unwrapError(rpcError : RpcError) : Error {
        switch rpcError {
            case (#ProviderError e) { e };
            case (#HttpOutcallError e) { e };
            case (#JsonRpcError e) { #JsonRpcError e };
            case (#ValidationError e) { e };
        };
    };

    public class Rpc(provider : Provider, source : Source) = this {

        public var defaultCycles = 1_000_000_000;

        func wrapActorSource(source : Source) : ActorSource {
            switch source {
                case (#Custom custom) { #Custom custom };
                case (#Service { hostname; network }) {
                    #Service {
                        hostname;
                        chainId = switch network {
                            case (?n) { ?wrapChainId(n) };
                            case null { null };
                        };
                    };
                };
                case (#Chain n) { #Chain(wrapChainId(n)) };
                case (#Provider n) { #Provider n };
            };
        };

        let actor_ = switch provider {
            case (#Canister a) { a };
            case (#Principal p) { actor (Principal.toText(p)) : RpcActor };
        };
        let actorSource = wrapActorSource(source);

        var nextId : Nat = 0;
        public func request(method : Text, params : JSON.JSON, maxResponseBytes : Nat64) : async Result<JSON.JSON> {
            nextId += 1;
            // prettier-ignore
            let payload = JSON.show(#Object([
                ("id", #Number(nextId)),
                ("jsonrpc", #String("2.0")),
                ("method", #String(method)),
                ("params", params),
            ]));
            switch (await requestPlain(payload, maxResponseBytes)) {
                case (#ok text) {
                    switch (JSON.parse(text)) {
                        case (?json) { #ok json };
                        case null {
                            #err(
                                #InvalidHttpJsonRpcResponse {
                                    status = 0;
                                    body = text;
                                    parsingError = ?("error while parsing JSON response");
                                }
                            );
                        };
                    };
                };
                case (#err err) { #err err };
            };
        };

        public func requestPlain(payload : Text, maxResponseBytes : Nat64) : async Result<Text> {
            func requestPlain_(payload : Text, maxResponseBytes : Nat64, cycles : Nat) : async Result<Text> {
                Cycles.add(cycles);
                switch (await actor_.request(actorSource, payload, maxResponseBytes)) {
                    case (#Ok ok) { #ok ok };
                    case (#Err err) { #err(unwrapError(err)) };
                };
            };
            switch (await requestPlain_(payload, maxResponseBytes, defaultCycles)) {
                case (#err(#TooFewCycles { expected })) {
                    debug {
                        Debug.print("Retrying with " # (debug_show expected) # " cycles");
                    };
                    await requestPlain_(payload, maxResponseBytes, expected);
                };
                case r r;
            };
        };
    };
};
