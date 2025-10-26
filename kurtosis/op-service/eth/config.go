package eth

import (
	"context"
	"encoding/json"
	"log/slog"
	"math/big"
	"net/http"

	"github.com/ethereum-optimism/optimism/op-service/client"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/log"
	"github.com/ethereum/go-ethereum/params"
)

// L1ChainConfigByChainID returns the chain config for the given chain ID,
// if it is in the set of known chain IDs (Mainnet, Sepolia, Holesky, Hoodi).
// If the chain ID is not known, it returns nil.
func L1ChainConfigByChainID(chainID ChainID) *params.ChainConfig {
	switch chainID {
	case ChainIDFromBig(params.MainnetChainConfig.ChainID):
		return params.MainnetChainConfig
	case ChainIDFromBig(params.SepoliaChainConfig.ChainID):
		return params.SepoliaChainConfig
	case ChainIDFromBig(params.HoleskyChainConfig.ChainID):
		return params.HoleskyChainConfig
	case ChainIDFromBig(params.HoodiChainConfig.ChainID):
		return params.HoodiChainConfig
	default:
		genesisTimestamp := getGenesis()
		return &params.ChainConfig{
			ChainID:                 chainID.ToBig(),
			HomesteadBlock:          big.NewInt(0),
			DAOForkBlock:            big.NewInt(0),
			DAOForkSupport:          true,
			EIP150Block:             big.NewInt(0),
			EIP155Block:             big.NewInt(0),
			EIP158Block:             big.NewInt(0),
			ByzantiumBlock:          big.NewInt(0),
			ConstantinopleBlock:     big.NewInt(0),
			PetersburgBlock:         big.NewInt(0),
			IstanbulBlock:           big.NewInt(0),
			MuirGlacierBlock:        big.NewInt(0),
			BerlinBlock:             big.NewInt(0),
			LondonBlock:             big.NewInt(0),
			ArrowGlacierBlock:       big.NewInt(0),
			GrayGlacierBlock:        big.NewInt(0),
			TerminalTotalDifficulty: big.NewInt(0),
			ShanghaiTime:            newUint64(0),
			CancunTime:              newUint64(0),
			PragueTime:              newUint64(0),
			OsakaTime:               newUint64(0),
			BPO1Time:                newUint64(0),
			// 2 epoch * 8 slot * 6 seconds
			BPO2Time:               newUint64(genesisTimestamp + 2*8*6),
			DepositContractAddress: common.HexToAddress("0x00000000219ab540356cbb839cbe05303d7705fa"),
			BlobScheduleConfig: &params.BlobScheduleConfig{
				Cancun: params.DefaultCancunBlobConfig,
				Prague: params.DefaultPragueBlobConfig,
				Osaka:  params.DefaultOsakaBlobConfig,
				BPO1:   params.DefaultBPO1BlobConfig,
				BPO2:   params.DefaultBPO2BlobConfig,
			},
		}
	}
}

func newUint64(val uint64) *uint64 { return &val }

func getGenesis() uint64 {
	logger := log.NewLogger(slog.Default().Handler())
	cl := client.NewBasicHTTPClient("http://cl-1-lodestar-geth:4000", logger)
	headers := http.Header{}
	headers.Add("Accept", "application/json")

	var genesisResp APIGenesisResponse
	resp, err := cl.Get(context.Background(), "eth/v1/beacon/genesis", nil, headers)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	if err = json.NewDecoder(resp.Body).Decode(&genesisResp); err != nil {
		panic(err)
	}
	return uint64(genesisResp.Data.GenesisTime)
}
