optimism_package = import_module("github.com/yoshidan/optimism-package/main.star")

def run(plan, args):
    # just delegate to optimism-package
    optimism_package.run(plan, args)
