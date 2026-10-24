# Core dependency build phases

## Problem

The Core image workflow for `fc45cee2d6709bd0d700c1a52a49bf3a416b4d91` exceeded the dependency cook's 240-second deadline. DataFusion was still compiling when the command exited with status 124. Docker cannot export a failed RUN layer, so retrying the same phase repeats its unfinished work.

## Design

Cook `datafusion-physical-plan`, then `datafusion`, then the full Core analytics dependency graph in successive Docker stages. Export each completed stage to the image variant's registry cache. Each following phase imports its predecessor's cache. The final Core cook remains responsible for the complete feature graph, including any dependencies whose feature selection differs from the earlier package builds.

Keep the existing 240-second cook deadlines, 285-second build wrapper and five-minute workflow steps. Preserve all image release verification and product design gates. This changes build preparation only; production memory remains 8 GB.

## Validation

Run workflow syntax validation and whitespace checks before pushing. Dispatch the Docker Build workflow against the PR branch to exercise the actual Linux image builds and release verification. A successful syntax check alone does not establish that compilation fits the deadlines. Deploy only a revision-verified image after the required gates pass.
