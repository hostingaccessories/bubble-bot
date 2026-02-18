# PHP {{ php_version }} runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    software-properties-common \
    && add-apt-repository ppa:ondrej/php -y \
    && apt-get update && apt-get install -y --no-install-recommends \
    php{{ php_version }}-cli \
    php{{ php_version }}-mbstring \
    php{{ php_version }}-xml \
    php{{ php_version }}-curl \
    php{{ php_version }}-zip \
    php{{ php_version }}-bcmath \
    php{{ php_version }}-intl \
    php{{ php_version }}-mysql \
    php{{ php_version }}-pgsql \
    php{{ php_version }}-sqlite3 \
    php{{ php_version }}-redis \
    php{{ php_version }}-gd \
    php{{ php_version }}-dom \
    php{{ php_version }}-tokenizer \
    && rm -rf /var/lib/apt/lists/*

COPY --from=composer:2 /usr/bin/composer /usr/bin/composer
