# Guide

Welcome to the Mocktioneer documentation. This guide will help you understand, configure, and deploy Mocktioneer for your testing needs.

## Quick Navigation

- [What is Mocktioneer?](./what-is-mocktioneer) - Understand what Mocktioneer does and why you might need it
- [Getting Started](./getting-started) - Set up and run Mocktioneer locally
- [Configuration](./configuration) - Learn about `edgezero.toml` and how to customize behavior
- [Architecture](./architecture) - Understand the crate structure and EdgeZero integration

## Adapters

Mocktioneer runs on multiple edge platforms:

- [Axum (Native)](./adapters/axum) - For local development and testing
- [Fastly Compute](./adapters/fastly) - Deploy to Fastly's edge network
- [Cloudflare Workers](./adapters/cloudflare) - Deploy to Cloudflare's edge network
