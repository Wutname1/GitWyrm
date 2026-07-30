# ai-provider Spec Delta

## ADDED Requirements

### Requirement: Several providers, one default

A user SHALL be able to have more than one AI provider configured at once, each with its
own selected model, and SHALL mark one as the default. Configuring the first provider SHALL
make it the default without the user being asked.

#### Scenario: Two providers configured

- WHEN a user has signed in to one provider and added an API key for another
- THEN Settings lists both as configured, each with its own model, and shows which is
  the default

#### Scenario: First provider

- WHEN a user configures their first provider
- THEN it becomes the default automatically, and the setting never has to be understood by
  a single-provider user

#### Scenario: Removing the default

- WHEN the default provider is removed
- THEN another configured provider becomes the default, and no feature is left pointing at
  a provider that is gone

#### Scenario: Removing the last provider

- WHEN the only configured provider is removed
- THEN every AI feature reports as not set up, exactly as it did before anything was
  configured

### Requirement: The default governs every AI feature

AI features SHALL use the default provider and its model. Commit-message generation, the
commit-generation flow, and Spec Desk runs SHALL all resolve the provider through one
shared path rather than reading the setting themselves, so they can never disagree about
which AI is in use.

#### Scenario: Switching the default

- WHEN the user changes which provider is the default
- THEN the next commit message, and the next run, use the new one - with no restart

#### Scenario: One answer everywhere

- WHEN the Spec Desk names the AI it would use
- THEN it names the same provider and model that commit-message generation would use

### Requirement: Per-provider model choice survives

Each configured provider SHALL keep its own selected model. Changing which provider is the
default SHALL NOT discard the model chosen for either.

#### Scenario: Switching back

- WHEN the user makes a second provider default and then switches back
- THEN the first provider still has the model it was set to
