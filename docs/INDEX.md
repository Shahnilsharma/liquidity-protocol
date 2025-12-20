# Documentation Index

Quick reference guide to all documentation in this project.

## Getting Started

1. **[README.md](../README.md)** - Start here
   - Project overview
   - Quick start guide
   - Contract interface
   - Usage examples

2. **[MIGRATION_GUIDE.md](../MIGRATION_GUIDE.md)** - If you used the old structure
   - Path changes
   - Command updates
   - Migration checklist

## Deployment

3. **[guides/DEPLOYMENT_GUIDE.md](guides/DEPLOYMENT_GUIDE.md)** - Complete deployment instructions
   - Prerequisites
   - ZigChain setup
   - Step-by-step deployment
   - Troubleshooting
   - Alternative testnets

4. **[PROJECT_STATUS.md](PROJECT_STATUS.md)** - Current deployment status
   - Deployed contracts
   - Testnet information
   - Current state
   - Next steps

## Testing

5. **[guides/queries.md](guides/queries.md)** - Manual testing guide
   - All query examples
   - Complete test flow
   - Phase-by-phase testing
   - Quick test scripts

6. **[guides/PHASE5_ERROR_TESTING_GUIDE.md](guides/PHASE5_ERROR_TESTING_GUIDE.md)** - Error case testing
   - Understanding test failures
   - Setup requirements
   - Testing scripts
   - Expected behaviors

7. **[TEST_RESULTS.md](TEST_RESULTS.md)** - Verification results
   - Test outcomes
   - Transaction hashes
   - Gas metrics
   - Production readiness

## Project Information

8. **[PROJECT_ORGANIZATION.md](PROJECT_ORGANIZATION.md)** - Structure guide
   - Directory layout
   - File descriptions
   - Relationships
   - Maintenance guidelines
   - Quick commands

9. **[REORGANIZATION_SUMMARY.md](REORGANIZATION_SUMMARY.md)** - What changed
   - Detailed change log
   - Before/after comparison
   - Benefits achieved
   - Verification checklist

10. **[../CHANGELOG.md](../CHANGELOG.md)** - Version history
    - Release notes
    - Changes by version
    - Migration notes

## Scripts

11. **[../scripts/deploy.sh](../scripts/deploy.sh)** - Automated deployment
    - ZigChain deployment
    - Contract uploads
    - Instantiation
    - Address persistence

12. **[../scripts/interact.sh](../scripts/interact.sh)** - Testing script
    - Balance checking
    - Deposit testing
    - Withdrawal testing
    - Pool queries

13. **[../scripts/QUICK_REFERENCE.sh](../scripts/QUICK_REFERENCE.sh)** - Command reference
    - Build commands
    - Test commands
    - Deployment alternatives

## By Use Case

### I want to...

**Deploy the contract**
1. Read: [guides/DEPLOYMENT_GUIDE.md](guides/DEPLOYMENT_GUIDE.md)
2. Run: `./scripts/deploy.sh`
3. Check: [PROJECT_STATUS.md](PROJECT_STATUS.md)

**Test the contract**
1. Read: [guides/queries.md](guides/queries.md)
2. Run: `./scripts/interact.sh`
3. Or follow manual steps in queries.md

**Understand the code**
1. Read: [../README.md](../README.md) - Overview
2. Read: [PROJECT_ORGANIZATION.md](PROJECT_ORGANIZATION.md) - Structure
3. Browse: `../src/` - Source code

**Verify functionality**
1. Read: [guides/queries.md](guides/queries.md)
2. Read: [guides/PHASE5_ERROR_TESTING_GUIDE.md](guides/PHASE5_ERROR_TESTING_GUIDE.md)
3. Check: [TEST_RESULTS.md](TEST_RESULTS.md)

**Migrate from old structure**
1. Read: [../MIGRATION_GUIDE.md](../MIGRATION_GUIDE.md)
2. Update paths in your scripts
3. Test with new commands

**Contribute to the project**
1. Read: [PROJECT_ORGANIZATION.md](PROJECT_ORGANIZATION.md)
2. Follow the structure guidelines
3. Update [../CHANGELOG.md](../CHANGELOG.md)

**Understand what changed**
1. Read: [REORGANIZATION_SUMMARY.md](REORGANIZATION_SUMMARY.md)
2. Read: [../CHANGELOG.md](../CHANGELOG.md)
3. Check: [../MIGRATION_GUIDE.md](../MIGRATION_GUIDE.md)

## Quick Links

### Source Code
- [src/contract.rs](../src/contract.rs) - Core logic
- [src/msg.rs](../src/msg.rs) - Message types
- [src/state.rs](../src/state.rs) - State management
- [src/error.rs](../src/error.rs) - Error definitions

### Configuration
- [Cargo.toml](../Cargo.toml) - Dependencies
- [Makefile](../Makefile) - Build automation
- [.gitignore](../.gitignore) - Git configuration

### Legal
- [../LICENSE](../LICENSE) - Apache 2.0
- [../NOTICE](../NOTICE) - Attribution

## Documentation Standards

All documentation in this project follows:
- ✅ Clear, professional language
- ✅ No decorative emojis
- ✅ Proper markdown formatting
- ✅ Working cross-references
- ✅ Step-by-step instructions
- ✅ Expected outputs included
- ✅ Troubleshooting sections

## Navigation Tips

**From project root:**
```bash
# View all docs
ls docs/

# View guides
ls docs/guides/

# Read a specific guide
cat docs/guides/DEPLOYMENT_GUIDE.md

# Search all docs
grep -r "keyword" docs/
```

**Quick commands:**
```bash
# Main readme
cat README.md

# Deployment
cat docs/guides/DEPLOYMENT_GUIDE.md

# Testing
cat docs/guides/queries.md

# Structure
cat docs/PROJECT_ORGANIZATION.md

# Changes
cat CHANGELOG.md
```

## Need Help?

1. Check this index for the right document
2. Read the relevant guide
3. Check troubleshooting sections
4. Review test results and examples
5. Examine source code comments

## Contributing Documentation

When adding new documentation:
1. Place in appropriate folder (docs/ or docs/guides/)
2. Follow existing style and formatting
3. Add entry to this index
4. Update cross-references
5. Update CHANGELOG.md
6. No emojis in professional docs

---

**Last Updated:** 2025-12-20
**Version:** 1.0.0
