# Licensing

How Cilica is licensed, and why. This page explains the model in plain terms. The licence files themselves are the binding text. They were drafted without a lawyer for a hobby project, so have them reviewed before Cilica earns real money.

## The short version

| Part | Licence | Who pays |
| --- | --- | --- |
| Desktop app (this repository) | [GNU AGPL v3](../LICENSE) (`AGPL-3.0-only`) | Nobody. Free for everyone, companies of any size included. |
| Server (sync, team management) | [Cilica Server License](../licenses/SERVER-LICENSE.md) (source available) | Organisations over 10 people, whether they self-host or use Cilica cloud. |
| Paid plans | [Cilica Commercial License](../licenses/COMMERCIAL-LICENSE.md) | Enterprise self-hosting, and companies that want the app without the AGPL. |
| Contributions | [Contributor License Agreement](../licenses/CLA.md) | Signed once by each outside contributor. |
| Name and logo | [Trademark policy](../TRADEMARKS.md) | Not licensed. Forks pick their own name. |

## Desktop app

The app is open source under the AGPL v3. Anyone can use, study, change and share it, for any purpose.

- **Forks stay open.** If you share a changed version, or let other people use one over a network, you must publish your source under the AGPL too. This stops closed rebrands.
- **Earlier versions.** Releases up to and including 0.9.0 were Apache 2.0 and stay available under those terms. The AGPL applies from the next release on.
- **Commercial option.** A company that can't accept the AGPL's obligations, for example because it wants to ship a closed, modified build, can buy the app under the commercial licence instead.

## Server

The server is what makes sync and team features work. Its source is public, so people can read, audit and self-host it, but it is source available, not open source: the licence limits who may use it for free. Never call the server "open source".

The server will live in its own repository. Copy `licenses/SERVER-LICENSE.md` there as its `LICENSE` file, along with the `Required Notice:` line.

### Free use

The Cilica Server License allows free use:

- **Personal.** For yourself, your household and people you know, as long as access isn't open to the public.
- **Small organisations.** Inside a company of 10 or fewer people, counting employees and contractors across the whole group of companies (parents, subsidiaries, sister companies).
- **Non-profits.** By charities, non-profit schools and universities, and public research organisations, whatever their size. Government bodies are not included and pay like companies.
- **Evaluation.** By anyone, for up to 30 days, to try it out.

A company that grows past 10 people gets 60 days to buy a licence or stop using the server.

### Paid use

An organisation over the limit can pick either:

- **Cilica cloud (Team plan).** We run the server. Billed per seat, under the Cilica Terms of Service.
- **Self-hosted (Enterprise plan).** They run the server on their own machines with a licence key, under the Cilica Commercial License. Priced higher than cloud.

### Never allowed

Nobody, of any size, may run the server so that other people or organisations can use it as a hosted service, whether or not they charge for it. Running it for your own organisation is fine. Removing or bypassing the licence key check is also not allowed.

## Where the licence text comes from

- **Server licence.** Adapted from the PolyForm Small Business and PolyForm Shield licences (version 1.0.0). The PolyForm Project allows anyone to adapt its licences, as long as an adapted version no longer mentions PolyForm, so the server licence doesn't. The hosted-service and licence-key limits are written in our own words, after the idea in the Elastic License 2.0.
- **Contributor agreement.** Written in our own words. It follows the structure of the Apache Individual CLA and the "any licence" option of the Harmony agreements: contributors keep their copyright, grant broad copyright and patent licences, and are promised their work stays available under its open source licence.
- **Trademark policy.** Written in our own words, following the usual structure of open source trademark policies.
- **Commercial licence.** Written in our own words. It covers the usual points of a software licence agreement.

## What a licence can't stop

The licences cover our code. Someone could still write their own sync server from scratch that works with the open source app. Copyright does not stop that. Two things limit it:

- **Trademark.** They cannot call it Cilica, Cilica Sync or anything confusingly similar, or use the logo.
- **Convenience.** The official path, built into the app, runs through us.

## To do

- [ ] Set up the CLA: paste `licenses/CLA.md` into a public GitHub Gist, sign in at <https://cla-assistant.io> with GitHub, and link the Gist to the `Benzo-Fury/cilica` repository. Do this before merging the first outside pull request.
- [ ] Write Cilica Terms of Service and a privacy policy before launching the cloud plan. These cover the hosted service, which none of the licences do.
- [ ] Before the server ships: create its repository with the server licence, and add the licence key check the commercial licence refers to.
- [ ] Optional: register "Cilica" as a trademark in the countries you sell in. Until then the name has only the weaker protection that comes from using it.
- [ ] Optional: if Cilica becomes a business, set up a company and transfer the copyright and trademarks to it. The CLA and the commercial licence already allow this.
