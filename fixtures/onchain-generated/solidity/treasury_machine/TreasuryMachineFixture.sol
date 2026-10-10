// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.36;

import {TreasuryMachineFcis} from "./TreasuryMachineFcis.sol";

contract TreasuryMachineFixture is TreasuryMachineFcis {
    function _commandAdmissible(
        Command memory command,
        Context memory
    ) internal pure override returns (bool) {
        return command.amount != 0
            && uint256(command.recipient) <= type(uint160).max;
    }

    function _invariant(
        State memory stateValue
    ) internal pure override returns (bool) {
        return stateValue.owner != bytes32(0);
    }

    function _decide(
        State memory stateValue,
        Command memory command,
        Context memory context
    ) internal pure override returns (Decision memory decision) {
        if (context.actor != stateValue.owner) {
            decision.kind = DecisionKind.Reject;
            decision.reasonCode = 1;
            return decision;
        }
        if (command.amount > stateValue.balance) {
            decision.kind = DecisionKind.Reject;
            decision.reasonCode = 2;
            return decision;
        }

        decision.kind = DecisionKind.Accept;
        decision.nextState = stateValue;
        decision.nextState.balance = stateValue.balance - command.amount;
        decision.eventCount = 1;
        decision.events[0] = _eventPaid(command.recipient, command.amount);
        decision.effectCount = 1;
        decision.effects[0] = _effectPayout(
            command.amount,
            stateValue,
            command,
            context
        );
    }
}
