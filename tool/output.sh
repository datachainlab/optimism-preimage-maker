agreed=$1
claimed=$2
agreed_hex=$(printf "0x%x" "$agreed")
claimed_hex=$(printf "0x%x" "$claimed")

# agreed
AGREED=$(curl -s -X POST http://localhost:9545 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\":\"2.0\",
    \"method\":\"optimism_outputAtBlock\",
    \"params\":[\"$agreed_hex\"],
    \"id\":2
  }")
AGREED_L2_HASH=$(echo $AGREED | jq .result.blockRef.hash)
echo "agreed l2 hash: $AGREED_L2_HASH"
AGREED_L2_OUTPUT=$(echo $AGREED | jq .result.outputRoot)
echo "agreed l2 output root: $AGREED_L2_OUTPUT"

# claimed
CLAIMED=$(curl -s -X POST http://localhost:9545 \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\":\"2.0\",
    \"method\":\"optimism_outputAtBlock\",
    \"params\":[\"$claimed_hex\"],
    \"id\":2
  }")
CLAIMED_L2_OUTPUT=$(echo $CLAIMED | jq .result.outputRoot)
echo "claimed l2 output root: $CLAIMED_L2_OUTPUT"

CLAIMED_L1_ORIGIN=$(echo $CLAIMED | jq .result.blockRef.l1origin.number)
L1_HEAD_NUMBER=$((CLAIMED_L1_ORIGIN+30))
echo "l1 head number: ${L1_HEAD_NUMBER}"
l1_head_num_hex=$(printf "0x%x" "$L1_HEAD_NUMBER")

L1_HEAD_HASH=$(curl -s -X POST localhost:8545 -d "{\"method\":\"eth_getBlockByNumber\", \"jsonrpc\": \"2.0\", \"id\":1, \"params\":[\"${l1_head_num_hex}\",false]}" -H "Content-Type: application/json" | jq .result.hash)
echo "l1_head_hash: ${L1_HEAD_HASH}"

echo "{ \"l1_head_hash\": ${L1_HEAD_HASH}, " > body.json
echo " \"agreed_l2_head_hash\": ${AGREED_L2_HASH}, " >> body.json
echo " \"agreed_l2_output_root\": ${AGREED_L2_OUTPUT}, " >> body.json
echo " \"l2_output_root\": ${CLAIMED_L2_OUTPUT}, " >> body.json
echo " \"l2_block_number\": ${claimed}} " >> body.json
