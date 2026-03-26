"""MARS Payments — Stripe Connect integration for GPU inference marketplace.

Handles:
- Provider onboarding (Stripe Express accounts)
- Consumer payments (charge per inference request)
- Platform fee collection (10%)
- Payout to providers

Usage:
    from mesh_protocol.payments import MeshPayments

    payments = MeshPayments(stripe_secret_key="sk_...")

    # Onboard a new GPU provider
    link = payments.create_provider_onboarding("provider@email.com")

    # Charge for an inference request
    charge = payments.charge_inference(
        provider_stripe_account="acct_xxx",
        amount_cents=15,  # $0.15
        description="Llama 3.3 70B — 1.5K tokens",
        consumer_payment_method="pm_xxx",
    )
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

logger = logging.getLogger(__name__)

PLATFORM_FEE_PERCENT = 10  # 10% platform fee


@dataclass
class ChargeResult:
    """Result of an inference charge."""
    success: bool
    charge_id: str
    amount_cents: int
    platform_fee_cents: int
    provider_payout_cents: int
    error: str | None = None


@dataclass
class OnboardingLink:
    """Stripe Connect onboarding link for a new provider."""
    url: str
    account_id: str


class MeshPayments:
    """Stripe Connect payment handler for the MARS GPU marketplace."""

    def __init__(self, stripe_secret_key: str, return_url: str = "https://mars-protocol.dev"):
        try:
            import stripe
            self.stripe = stripe
            self.stripe.api_key = stripe_secret_key
        except ImportError:
            raise ImportError("stripe package required: pip install stripe")

        self.return_url = return_url

    def create_provider_account(self, email: str) -> OnboardingLink:
        """Create a Stripe Express account for a new GPU provider.

        Returns an onboarding link the provider must visit to complete setup.
        """
        account = self.stripe.Account.create(
            type="express",
            email=email,
            capabilities={
                "transfers": {"requested": True},
            },
            metadata={"platform": "mars-protocol", "role": "gpu-provider"},
        )

        link = self.stripe.AccountLink.create(
            account=account.id,
            refresh_url=f"{self.return_url}/onboarding/refresh",
            return_url=f"{self.return_url}/onboarding/complete",
            type="account_onboarding",
        )

        logger.info("Created provider account %s for %s", account.id, email)
        return OnboardingLink(url=link.url, account_id=account.id)

    def check_provider_status(self, account_id: str) -> dict[str, Any]:
        """Check if a provider's Stripe account is ready to receive payments."""
        account = self.stripe.Account.retrieve(account_id)
        return {
            "account_id": account.id,
            "charges_enabled": account.charges_enabled,
            "payouts_enabled": account.payouts_enabled,
            "details_submitted": account.details_submitted,
        }

    def charge_inference(
        self,
        provider_stripe_account: str,
        amount_cents: int,
        description: str,
        consumer_payment_method: str,
        consumer_email: str | None = None,
    ) -> ChargeResult:
        """Charge a consumer for an inference request and route payment to provider.

        Uses Stripe Connect destination charges:
        - Consumer is charged the full amount
        - Platform keeps 10% as fee
        - Provider receives 90% as payout
        """
        if amount_cents < 50:
            # Stripe minimum is $0.50 — batch small charges or use credits
            return ChargeResult(
                success=False,
                charge_id="",
                amount_cents=amount_cents,
                platform_fee_cents=0,
                provider_payout_cents=0,
                error=f"Amount too small ({amount_cents}c). Minimum charge is 50 cents. Use MU credits for micropayments.",
            )

        platform_fee = max(1, amount_cents * PLATFORM_FEE_PERCENT // 100)
        provider_amount = amount_cents - platform_fee

        try:
            payment_intent = self.stripe.PaymentIntent.create(
                amount=amount_cents,
                currency="usd",
                payment_method=consumer_payment_method,
                confirm=True,
                description=description,
                application_fee_amount=platform_fee,
                transfer_data={
                    "destination": provider_stripe_account,
                },
                metadata={
                    "platform": "mars-protocol",
                    "provider_account": provider_stripe_account,
                },
                **({"receipt_email": consumer_email} if consumer_email else {}),
            )

            logger.info(
                "Charged %dc (fee=%dc, provider=%dc) → %s",
                amount_cents, platform_fee, provider_amount, provider_stripe_account,
            )

            return ChargeResult(
                success=True,
                charge_id=payment_intent.id,
                amount_cents=amount_cents,
                platform_fee_cents=platform_fee,
                provider_payout_cents=provider_amount,
            )

        except self.stripe.StripeError as e:
            logger.error("Stripe charge failed: %s", e)
            return ChargeResult(
                success=False,
                charge_id="",
                amount_cents=amount_cents,
                platform_fee_cents=0,
                provider_payout_cents=0,
                error=str(e),
            )

    def estimate_cost(
        self,
        price_per_mtok: float,
        estimated_tokens: int,
    ) -> dict[str, int]:
        """Estimate the cost of an inference request in cents.

        price_per_mtok is in USD per million tokens (e.g. 2.50 = $2.50/M tokens).
        """
        total_cents = int(price_per_mtok * estimated_tokens / 1_000_000 * 100)
        platform_fee = max(1, total_cents * PLATFORM_FEE_PERCENT // 100)
        return {
            "total_cents": total_cents,
            "platform_fee_cents": platform_fee,
            "provider_payout_cents": total_cents - platform_fee,
        }
