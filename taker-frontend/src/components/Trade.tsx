import { BoxProps } from "@chakra-ui/layout";
import {
    Box,
    Button,
    Center,
    Circle,
    FormControl,
    FormHelperText,
    FormLabel,
    Grid,
    GridItem,
    HStack,
    IconButton,
    InputGroup,
    NumberDecrementStepper,
    NumberIncrementStepper,
    NumberInput,
    NumberInputField,
    NumberInputStepper,
    Skeleton,
    Slider,
    SliderFilledTrack,
    SliderMark,
    SliderThumb,
    SliderTrack,
    Table,
    Tbody,
    Td,
    Text,
    Tooltip,
    Tr,
    useColorModeValue,
    useDisclosure,
    VStack,
} from "@chakra-ui/react";
import { motion } from "framer-motion";
import * as React from "react";
import { useEffect, useState } from "react";
import { FaWallet } from "react-icons/all";
import { useNavigate } from "react-router-dom";
import { Offer } from "../App";
import { CfdOrderRequestPayload, ConnectionStatus } from "../types";
import usePostRequest from "../usePostRequest";
import AlertBox from "./AlertBox";
import BitcoinAmount from "./BitcoinAmount";
import ConfirmOrderModal from "./ConfirmOrderModal";
import DollarAmount from "./DollarAmount";
import { FundingRateTooltip } from "./FundingRateTooltip";

const MotionBox = motion<BoxProps>(Box);

// TODO: Consider inlining the Trade code in App, there is not much value in this abstraction anymore
//  Recommendation: Inline, see how it feels and then potentially carve out some new abstraction if there is one clearly visible
interface TradeProps {
    offer: Offer;
    connectedToMaker: ConnectionStatus;
    walletBalance: number;
    isLong: boolean;
}

export default function Trade({
    offer: {
        id: orderId,
        price: priceAsNumber,
        fundingRateAnnualized,
        fundingRateHourly,
        minQuantity,
        maxQuantity,
        lotSize,
        leverageDetails,
    },
    connectedToMaker,
    walletBalance,
    isLong,
}: TradeProps) {
    const navigate = useNavigate();

    let [quantity, setQuantity] = useState(0);
    let [leverage, setLeverage] = useState(2);
    let [userHasEdited, setUserHasEdited] = useState(false);

    const currentLeverageDetails = leverageDetails.find((leverageDetail) => leverageDetail.leverage === leverage);
    const leverageChoices = leverageDetails.map((leverageDetail) => leverageDetail.leverage);

    // We update the quantity because the offer can change any time.
    useEffect(() => {
        if (!userHasEdited) {
            setQuantity(minQuantity);
        }
    }, [userHasEdited, minQuantity, setQuantity]);

    let [onSubmit, isSubmitting] = usePostRequest<CfdOrderRequestPayload>("/api/cfd/order");

    let outerCircleBg = useColorModeValue("gray.100", "gray.700");
    let innerCircleBg = useColorModeValue("gray.200", "gray.600");

    const { isOpen, onOpen, onClose } = useDisclosure();

    const margin = (quantity / lotSize) * (currentLeverageDetails?.margin_per_lot || 0);
    const feeForFirstSettlementInterval = (quantity / lotSize)
        * (currentLeverageDetails?.initial_funding_fee_per_lot || 0);
    const balanceTooLow = walletBalance < margin;

    const quantityTooHigh = maxQuantity < quantity;
    const quantityTooLow = minQuantity > quantity;
    const quantityGreaterZero = quantity > 0;
    const quantityIsEvenlyDivisibleByIncrement = isEvenlyDivisible(quantity, lotSize);

    const canSubmit = orderId && !balanceTooLow && !isSubmitting && !quantityTooHigh && !quantityTooLow
        && quantityGreaterZero
        && quantityIsEvenlyDivisibleByIncrement;

    let alertBox;

    if (connectedToMaker.online) {
        if (balanceTooLow) {
            alertBox = (
                <AlertBox
                    title={"Not enough balance to open a new position!"}
                    description={"Deposit more into your wallet."}
                    status={"warning"}
                    reachLinkTo={"/wallet"}
                />
            );
        }
        if (!quantityIsEvenlyDivisibleByIncrement) {
            alertBox = (
                <AlertBox
                    title={`Quantity is not in increments of ${lotSize}!`}
                    description={`Increment is ${lotSize}`}
                />
            );
        }
        if (quantityTooHigh) {
            alertBox = (
                <AlertBox
                    title={"Quantity too high!"}
                    description={`Max available liquidity is ${maxQuantity}`}
                />
            );
        }
        if (quantityTooLow || !quantityGreaterZero) {
            alertBox = <AlertBox title={"Quantity too low!"} description={`Min quantity is ${minQuantity}`} />;
        }
        if (!orderId) {
            alertBox = (
                <AlertBox
                    title={"Limited liquidity in maker!"}
                    description={"The maker you are connected has no active offers"}
                    status={"warning"}
                />
            );
        }
    }

    return (
        <VStack>
            ?<Center>
                <Grid
                    templateRows="repeat(1, 1fr)"
                    templateColumns="repeat(1, 1fr)"
                    gap={4}
                    maxWidth={"500px"}
                >
                    <GridItem colSpan={1}>
                        <Center>
                            <MotionBox
                                variants={{
                                    pulse: {
                                        scale: [1, 1.05, 1],
                                    },
                                }}
                                // @ts-ignore: lint is complaining but should be fine :)
                                transition={{
                                    // type: "spring",
                                    ease: "linear",
                                    duration: 2,
                                    repeat: Infinity,
                                }}
                                animate={"pulse"}
                                id={isLong ? "makerLongPrice" : "makerShortPrice"}
                            >
                                <Circle size="256px" bg={outerCircleBg}>
                                    <Circle size="180px" bg={innerCircleBg}>
                                        <MotionBox>
                                            <VStack>
                                                <Skeleton isLoaded={!!priceAsNumber && priceAsNumber > 0}>
                                                    <Text fontSize={"4xl"} as="b">
                                                        <DollarAmount amount={priceAsNumber || 0} />
                                                    </Text>
                                                </Skeleton>
                                            </VStack>
                                        </MotionBox>
                                    </Circle>
                                </Circle>
                            </MotionBox>
                        </Center>
                    </GridItem>
                    <GridItem colSpan={1} paddingLeft={5} paddingRight={5}>
                        <Quantity
                            min={minQuantity}
                            max={maxQuantity}
                            quantity={quantity}
                            onChange={(_valueAsString: string, valueAsNumber: number) => {
                                setQuantity(Number.isNaN(valueAsNumber) ? 0 : valueAsNumber);
                                setUserHasEdited(true);
                            }}
                            lotSize={lotSize}
                            isLong={isLong}
                        />
                    </GridItem>
                    <GridItem colSpan={1} paddingLeft={5} paddingRight={5}>
                        <Leverage
                            leverage_choices={leverageChoices}
                            currentChoice={leverage}
                            onChange={setLeverage}
                            isLong={isLong}
                        />
                    </GridItem>
                    <GridItem colSpan={1}>
                        <Table variant="simple">
                            <Tbody>
                                <Tr id={isLong ? "longRequiredMargin" : "shortRequiredMargin"}>
                                    <Td>Required Margin</Td>
                                    <Td isNumeric>
                                        <BitcoinAmount btc={margin} />
                                    </Td>
                                </Tr>
                                <Tr>
                                    <Td>
                                        <HStack>
                                            <Text>Available Balance</Text>
                                            <Tooltip label={"Jump to wallet"} hasArrow>
                                                <IconButton
                                                    variant={"unstyled"}
                                                    aria-label="Go to wallet"
                                                    icon={<FaWallet />}
                                                    onClick={() => navigate("/wallet")}
                                                />
                                            </Tooltip>
                                        </HStack>
                                    </Td>
                                    <Td isNumeric>
                                        <BitcoinAmount btc={walletBalance} />
                                    </Td>
                                </Tr>

                                <Tr id={isLong ? "longPerpetualCost" : "shortPerpetualCost"}>
                                    <Td>
                                        <Text>Perpetual Cost</Text>
                                    </Td>
                                    <FundingRateTooltip
                                        fundingRateHourly={fundingRateHourly}
                                        fundingRateAnnualized={fundingRateAnnualized}
                                        disabled={!fundingRateHourly}
                                    >
                                        <Td isNumeric>
                                            <Skeleton isLoaded={fundingRateHourly != null}>
                                                Hourly @ {fundingRateHourly}%
                                            </Skeleton>
                                        </Td>
                                    </FundingRateTooltip>
                                </Tr>
                            </Tbody>
                        </Table>
                    </GridItem>
                    <GridItem colSpan={1}>
                        <Center>
                            <Button
                                disabled={!canSubmit}
                                colorScheme={isLong ? "green" : "red"}
                                size="lg"
                                onClick={onOpen}
                                h={16}
                                w={"80%"}
                                id={isLong ? "longButton" : "shortButton"}
                            >
                                <VStack>
                                    <Text fontSize={"md"}>{isLong ? "Long" : "Short"}</Text>
                                    <Text fontSize={"sm"}>{`@ ${priceAsNumber || "no price"}`}</Text>
                                </VStack>
                            </Button>
                            <ConfirmOrderModal
                                orderId={orderId!}
                                position={isLong ? "long" : "short"}
                                price={priceAsNumber || 0}
                                isOpen={isOpen}
                                onClose={onClose}
                                isSubmitting={isSubmitting}
                                onSubmit={onSubmit}
                                quantity={quantity}
                                margin={margin}
                                leverage={leverage}
                                liquidationPriceAsNumber={currentLeverageDetails?.liquidation_price || 0}
                                feeForFirstSettlementInterval={feeForFirstSettlementInterval}
                                fundingRateHourly={fundingRateHourly || 0}
                                fundingRateAnnualized={fundingRateAnnualized || 0}
                            />
                        </Center>
                    </GridItem>
                </Grid>
            </Center>
            {alertBox}
        </VStack>
    );
}

interface QuantityProps {
    min: number;
    max: number;
    quantity: number;
    lotSize: number;
    onChange: (valueAsString: string, valueAsNumber: number) => void;
    isLong: boolean;
}

function Quantity({ min, max, onChange, quantity, lotSize, isLong }: QuantityProps) {
    return (
        <FormControl id="quantity">
            <Center>
                <FormLabel>BTC/USD Contracts</FormLabel>
            </Center>
            <InputGroup id={isLong ? "longQuantityInput" : "shortQuantityInput"}>
                <NumberInput
                    min={min}
                    max={max}
                    defaultValue={min}
                    step={lotSize}
                    onChange={onChange}
                    value={quantity}
                    w={"100%"}
                >
                    <NumberInputField />
                    <NumberInputStepper>
                        <NumberIncrementStepper />
                        <NumberDecrementStepper />
                    </NumberInputStepper>
                </NumberInput>
            </InputGroup>
            <FormHelperText>How much do you want to buy or sell?</FormHelperText>
        </FormControl>
    );
}

interface LeverageProps {
    leverage_choices: number[];
    currentChoice: number;
    onChange: (val: number) => void;
    isLong: boolean;
}

function Leverage({ leverage_choices, onChange, currentChoice, isLong }: LeverageProps) {
    const min = Math.min.apply(Math, leverage_choices);
    const max = Math.max.apply(Math, leverage_choices);

    return (
        <Box id={isLong ? "longLeverage" : "shortLeverage"}>
            <FormControl id="leverage">
                <Center>
                    <FormLabel>Leverage</FormLabel>
                </Center>
                <Slider
                    value={currentChoice}
                    min={min}
                    max={max}
                    onChange={(val) => onChange(val)}
                    onChangeEnd={(val) => onChange(val)}
                >
                    {leverage_choices.map(leverage => <SliderMark key={leverage} value={leverage} fontSize="sm" />)}
                    <SliderTrack>
                        <Box position="relative" right={10} />
                        <SliderFilledTrack />
                    </SliderTrack>
                    <SliderThumb boxSize={6}>
                        <Text color="black">{currentChoice}</Text>
                    </SliderThumb>
                </Slider>
                <FormHelperText>
                    How much do you want to leverage your position?
                </FormHelperText>
            </FormControl>
        </Box>
    );
}

export function isEvenlyDivisible(numerator: number, divisor: number): boolean {
    return (numerator % divisor === 0.0);
}
