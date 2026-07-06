from __future__ import annotations

import pytest

from xray_fluent import happ_crypt
from xray_fluent.happ_crypt import (
    HappDecryptError,
    HappKeyUnavailableError,
    decrypt_happ_link,
    is_happ_crypt_link,
    is_happ_link,
)

# Публичные тестовые векторы (ключи Happ извлечены сообществом; ссылки
# расшифровываются в реальный URL подписки). Позволяют проверить весь конвейер
# перестановок + RSA-PKCS1v15 + ChaCha20-Poly1305 без сети.
CRYPT4_LINK = (
    "happ://crypt4/LOlGv0ZXi8lPDPNEPT4NjoA5GOck+iV4io1Rhmd8GS13HmQ0h7mHwylUdicX6/JFvXe"
    "Aq/H/XoHbYNU1DT9pVaUjY82tmTqh42FkxZ5GzHmu45tobtPeM5fjabS3JcGTiNVO/a8YtBhpcnLFD/wZ7"
    "Ie3koAJlrWXUDmeDAxLsL649WLBE0JtN3Yehnsxh+0MG8BHSvUQDrxAW5X4A6JvRvGjZ2Nt/vvSuLQNrY8"
    "intgYlcATaDNhAcGZWIcXESe6sf8CGTbY5KIRmr2+uBERoDOvulDtHzeZxUxODoq3qPbVjURI5vUYm6o4p5"
    "KAaTDPQG2ZbJWA2uEsOogbaRCo9oxIkF/vMIBMd5IKy6KQd4Ug6KR0qqHByhcQtJc3CcPQnix7dDYLYEcnK"
    "0qP+eCYMtdLl4+o4eKPrmx5dPPdrKcp83SOvhYbm9g6MGlyqyCfh8IdO5zfGQB6MnjTzpRUKan32iFiuTBP"
    "DzFOL1aAyoA17/ZloRG+jVUYPNjqxczvUxPojruZkmA0I9FJFL/zgtE5FAUd7WBHTwBkSKHOEiPMePZfHiz"
    "P+J22ZlSgSCnTOiwcyKYGiQLf7TbKsuUmqn29zidStjmMkKOEkjk21yuiD6QUDnZnGko79Jg67m3/hk4/km"
    "12ZOqH9V64T+p67/NqR0/KVIXA/jrvbtL4H2s="
)
CRYPT4_EXPECTED = "https://premiumt.shop/sub/5ESXeShpoSc_mbKK"

CRYPT5_LINK = (
    "happ://crypt5/neirLBO3s2Y9dNfS0s14I20jIyBax2hdTEBzyJCM4og3aIxdvMC8+ocHYSeouvAtcztQ"
    "V6TogDHy/CFp9KokhGguo/KptKbd4haxc6AwQOA7cT1nmGOhaXwOBS2PrUPIllVeS2wwMUGeCSI/9CfO0lS"
    "B4Wd70=rybuRkSgvpp+gKKJLC2sFFqH4VOwalpFPy2HFcpebqEaoFGG5xsp6BmAxaoseVfuiDZx1Y7qbv9J"
    "dBB1jWV17sVU7PQLkcSxlQA9/NLerxQfSWFQUBPgwroA0QAyLxaqc43GJHhZl0ozxhZ2LqnEVXJ+7186i78"
    "l4RI43qvazSzSY78k3hh6dcxwwVS9l/vBeSu1gWPp606cRDOwnR8f50WQ1zY+/hzkBfJDX59tLxwhV4c9ZK"
    "fIOTQdzW7sYqp2BO5QacpMZZDMc+u/m/RHkrKwIJVUhupSNHQ2nxntXW/i8FGlm8NCg21dSQ+go29N6tef9"
    "iTeU6+jugQ5c620uOuY5VzH99G1V6oZtWooCcdIiStC1GFB8cXHb5Q0uDaf288YporSMR7BJwlAgfa1ry/v"
    "cd9IDrGXFvhgITSw8BmAKlSK8B/84SxAbgEBZdaBR7I7+MQG/5VY8VKTA8aFSAy/N+e5NVeRoGZZKj2+bQh"
    "x2jic8CeVIuHV8XKpNEQfceSQhpIutyWsMSnG9SEWzxjGTs3sDvjSA/B1j3uMK2HAR+WfHXfOALjQD2shSI"
    "5GtomGoKZqD147uqbUlTGwdI4FCBAxg28rJh2zG+5CRC33R9VwLh3oMQuIzNn8BujyGfV5MNPa6A5diRrNv"
    "pLUpVIM3yUgYTrQ2+VUA4h4ibg=Abftjv"
)
CRYPT5_EXPECTED = "https://ph4nt0m.megafaber.ru/sub/djMsMTA1LDE3ODAyMzg0NjIf6ec469212"

# Ссылка из задачи — формат crypt5.1, ключ отсутствует в публичном наборе.
CRYPT51_LINK = (
    "happ://crypt5/fzvd4oXqWHPd9ZJzbmZcpU3I20FsDc8WfLpIJg8yO6G9p/GbNqkmpD1avm2fTYWsJmVe"
    "Kxs/zdzR8yugTK73iSH6DXZ+Z/U6KivYcEeNBtYcSrziaK5+PDLsBMsCL1qwyDpXGn3esHXxj9tXNE/t0mm"
    "HiJycS6n6B3TnrpXNsBcpEUgji9oORF46JK0i5xwpAXrDNqY/4hLaGJhK0X4hoFkyuqx8M1VKXabyVq9q0g"
    "eu84PwTPH2FOeOh1rKmFNWTMMcOSPG2YjFg6phIgEpoks8fwystrTVWV3138pqmeMRwzYthQcatqxRRMsrc"
    "wGnhq4mymB813vPboFGHflMcyYT/hpWAz9WfPPWjldEfgMhLHiS0+mznmZsHY9n9ZFU8gMHDtbIJTirbukv"
    "6V2taTh6wan4a6FWKovf85mIO6iUYbpQE3Uz3czKldiBx/MEFfTA5/k9N3WC1MQG2LddZ6Vod6thWpwaN7/"
    "ZhgqoHflA1hoV0SDaQ0q+EWI+egMoFrsRs55E91r1yObG5uYw9OZ399Qtv3ecveX98YOF4k8cn0DLrYhm7i"
    "CrbpWwLeg4bCFIY9KTq+u1TAqNIKxMlm29Tb2tSMFu7zoypz+GacEl00y4lTHpm/FTQtbqHxSSz7GCVYepZ"
    "XfJkxQkjMf9V53YyrYtsbGhw8mhUnHOtEg0L/kHldlqpRqGctgvA1aA7OzpviIoyYv6BvqxblSBQrYIRZEj"
    "1WPE8P+rNodlI+6jMC16QFW/b2NWUtzuz7U8+slkCHdTV20hv+GZ6nIap41RKp41OPi5Un+PTkfGailpGaz"
    "GInwecp8DXYuvudSxZqIeopf8YODcle1iWnSUJkurlnNP55jlmwCffr9c70mf7B+Q6OtMfb/f7rL8p3DjQL"
    "mzW/Cv+q0l2nCpqAxYM1+Nfos=ff"
)


def test_detects_happ_links() -> None:
    assert is_happ_link("happ://crypt5/AAAA")
    assert is_happ_crypt_link("happ://crypt4/AAAA")
    assert is_happ_crypt_link("HAPP://CRYPT/AAAA")  # регистронезависимо
    assert not is_happ_crypt_link("happ://add/aHR0cHM6Ly94")
    assert not is_happ_crypt_link("https://example.com/sub")


def test_decrypts_public_crypt4_vector() -> None:
    assert decrypt_happ_link(CRYPT4_LINK) == CRYPT4_EXPECTED


def test_decrypts_public_crypt5_vector() -> None:
    assert decrypt_happ_link(CRYPT5_LINK) == CRYPT5_EXPECTED


def test_crypt51_without_public_key_raises_clear_error() -> None:
    with pytest.raises(HappKeyUnavailableError) as exc:
        decrypt_happ_link(CRYPT51_LINK)
    assert "crypt5.1" in str(exc.value)
    assert "vdfzfoff" in str(exc.value)


def test_unsupported_scheme_raises() -> None:
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("happ://routing/whatever")
    with pytest.raises(HappDecryptError):
        decrypt_happ_link("https://not-a-happ-link")


def test_swap_pairs_and_block_pair_swap_round_trip_ascii() -> None:
    # swap_pairs — своя инверсия; block_pair_swap применённый дважды к строке
    # кратной 4 — тоже возвращает исходную (CDAB → CDAB → ABCD).
    assert happ_crypt._swap_pairs(happ_crypt._swap_pairs("ABCDE")) == "ABCDE"
    assert happ_crypt._block_pair_swap(happ_crypt._block_pair_swap("ABCDEFGH")) == "ABCDEFGH"
