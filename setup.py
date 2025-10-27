from setuptools import setup

# Note: This package uses maturin for building, so this setup.py is provided for compatibility
# The actual build is handled by pyproject.toml and maturin

setup(
    name="promptpro",
    version="0.1.0",
    description="PromptPro - A prompt versioning and management system with Python bindings",
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    author="jinfagang",
    author_email="jinfagang@163.com",
    url="https://github.com/lucasjinreal/promptpro",
    license="GPL-3.0-only",
    packages=["promptpro"],
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: GNU General Public License v3 (GPLv3)",
        "Operating System :: OS Independent",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Topic :: Utilities"
    ],
    python_requires=">=3.8",
    install_requires=[],
    zip_safe=False,
)