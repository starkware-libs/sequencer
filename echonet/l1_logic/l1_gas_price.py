from echonet.constants import BPO2_UPDATE_FRACTION, MAX_EXCESS_BLOB_GAS_SEARCH


class L1GasPrice:
    @staticmethod
    def bpo2_blob_fee_wei(excess_blob_gas: int) -> int:
        """
        Return the blob base fee in wei for a given `excess_blob_gas` under the BPO2 parameter.
        The fee grows exponentially as excess blob gas grows.
        BPO2 (Blob Parameter Option 2) is the blob configuration defined by EIP-7892.
        This matches `eip7840::BlobParams::bpo2().calc_blob_fee` in `crates/papyrus_base_layer/src/ethereum_base_layer_contract.rs`.
        """
        denominator = BPO2_UPDATE_FRACTION
        i = 1
        output = 0
        accumulator = denominator
        while accumulator > 0:
            output += accumulator
            accumulator = (accumulator * excess_blob_gas) // (denominator * i)
            i += 1
        return output // denominator

    @staticmethod
    def excess_blob_gas_for_fee(target_blob_fee_wei: int) -> int:
        """
        Inverse of `bpo2_blob_fee_wei`.
        Given a target blob fee, returns an `excess_blob_gas` value whose computed fee
        equals that target (or the closest fee below it if the target falls between two
        fee steps). Due to integer floor division, many excess_blob_gas values map to
        the same fee; this returns the smallest such value (the start of the plateau).
        This is sufficient for echonet's use case: the value is only passed back through
        `bpo2_blob_fee_wei`, which gives the same result for any point on the plateau.
        """
        if target_blob_fee_wei <= 1:
            return 0
        low, high = 0, MAX_EXCESS_BLOB_GAS_SEARCH
        while low < high:
            mid = (low + high) // 2
            if L1GasPrice.bpo2_blob_fee_wei(mid) < target_blob_fee_wei:
                low = mid + 1
            else:
                high = mid
        if L1GasPrice.bpo2_blob_fee_wei(low) == target_blob_fee_wei:
            return low
        return low - 1
