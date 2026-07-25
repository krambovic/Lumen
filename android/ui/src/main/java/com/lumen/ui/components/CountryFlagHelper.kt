package com.lumen.ui.components

import androidx.compose.ui.graphics.Color
import java.util.Locale
import java.util.regex.Pattern

enum class StripeStyle {
    Horizontal,
    Vertical,
    Nordic,
    Cross,
    // Detailed flags: plain stripes cannot represent these at all.
    UsStars,
    UnionJack,
    Disc,
    CnStars,
    Crescent,
    MapleLeaf,
    Taegeuk
}

data class FlagStripeData(
    val style: StripeStyle,
    val colors: List<Color>
)

object CountryFlagHelper {

    // Stripe definitions matching country_flags.py
    val STRIPES: Map<String, FlagStripeData> = mapOf(
        // Horizontal tri-stripes
        "RU" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF0039A6), Color(0xFFD52B1E))),
        "DE" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF000000), Color(0xFFDD0000), Color(0xFFFFCC00))),
        "NL" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFAE1C28), Color(0xFFFFFFFF), Color(0xFF21468B))),
        "LU" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFED2939), Color(0xFFFFFFFF), Color(0xFF00A1DE))),
        "AT" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFED2939), Color(0xFFFFFFFF), Color(0xFFED2939))),
        "HU" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCE2939), Color(0xFFFFFFFF), Color(0xFF477050))),
        "BG" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF00966E), Color(0xFFD62612))),
        "LT" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFDB913), Color(0xFF006A44), Color(0xFFC1272D))),
        "EE" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF0072CE), Color(0xFF000000), Color(0xFFFFFFFF))),
        "LV" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF9E3039), Color(0xFFFFFFFF), Color(0xFF9E3039))),
        "HR" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFF0000), Color(0xFFFFFFFF), Color(0xFF171796))),
        "RS" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFC7363C), Color(0xFF0C4076), Color(0xFFFFFFFF))),
        "SI" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF003DA5), Color(0xFFED1C24))),
        "AM" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFD90012), Color(0xFF0033A0), Color(0xFFF2A800))),
        "AZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF00B5E2), Color(0xFFDD0000), Color(0xFF00B532))),
        "CO" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFCD116), Color(0xFF003893), Color(0xFFCE1126))),
        "AR" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF74ACDF), Color(0xFFFFFFFF), Color(0xFF74ACDF))),
        "IN" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFF9933), Color(0xFFFFFFFF), Color(0xFF138808))),
        "EG" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCE1126), Color(0xFFFFFFFF), Color(0xFF000000))),
        "IR" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF239F40), Color(0xFFFFFFFF), Color(0xFFDA0000))),
        "BO" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFD52B1E), Color(0xFFF9E300), Color(0xFF007934))),
        "ET" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF009A44), Color(0xFFFCDD09), Color(0xFFDA121A))),
        "YE" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCE1126), Color(0xFFFFFFFF), Color(0xFF000000))),
        "IQ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCE1126), Color(0xFFFFFFFF), Color(0xFF000000))),
        "TJ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCC0000), Color(0xFFFFFFFF), Color(0xFF006600))),
        "GH" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCF0921), Color(0xFFFCD116), Color(0xFF006B3F))),

        // Horizontal bi-stripes
        "UA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF005BBB), Color(0xFFFFD500))),
        "PL" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFFDC143C))),
        "ID" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFF0000), Color(0xFFFFFFFF))),
        "MC" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFCE1126), Color(0xFFFFFFFF))),
        "SG" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFEF3340), Color(0xFFFFFFFF))),

        // Vertical tri-stripes
        "FR" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF002395), Color(0xFFFFFFFF), Color(0xFFED2939))),
        "IT" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF009246), Color(0xFFFFFFFF), Color(0xFFCE2B37))),
        "IE" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF009A49), Color(0xFFFFFFFF), Color(0xFFFF7900))),
        "BE" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF000000), Color(0xFFFAE042), Color(0xFFED2939))),
        "RO" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF002B7F), Color(0xFFFCD116), Color(0xFFCE1126))),
        "MD" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF003DA5), Color(0xFFFCD116), Color(0xFFCC0000))),
        "MX" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF006847), Color(0xFFFFFFFF), Color(0xFFCE1126))),
        "NG" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF008751), Color(0xFFFFFFFF), Color(0xFF008751))),
        "CI" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFFF77F00), Color(0xFFFFFFFF), Color(0xFF009E60))),
        "PE" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFFD91023), Color(0xFFFFFFFF), Color(0xFFD91023))),
        "CA" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFFFF0000), Color(0xFFFFFFFF), Color(0xFFFF0000))),
        "PT" to FlagStripeData(StripeStyle.Vertical, listOf(Color(0xFF006600), Color(0xFFFF0000), Color(0xFFFF0000))),

        // Cross & Nordic flags
        "SE" to FlagStripeData(StripeStyle.Nordic, listOf(Color(0xFF006AA7), Color(0xFFFECC02))),
        "NO" to FlagStripeData(StripeStyle.Nordic, listOf(Color(0xFFEF2B2D), Color(0xFFFFFFFF), Color(0xFF002868))),
        "FI" to FlagStripeData(StripeStyle.Nordic, listOf(Color(0xFFFFFFFF), Color(0xFF003580))),
        "DK" to FlagStripeData(StripeStyle.Nordic, listOf(Color(0xFFC60C30), Color(0xFFFFFFFF))),
        "IS" to FlagStripeData(StripeStyle.Nordic, listOf(Color(0xFF003897), Color(0xFFFFFFFF), Color(0xFFD72828))),
        "CH" to FlagStripeData(StripeStyle.Cross, listOf(Color(0xFFFF0000), Color(0xFFFFFFFF))),
        "GR" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF0D5EAF), Color(0xFFFFFFFF), Color(0xFF0D5EAF))),
        "GE" to FlagStripeData(StripeStyle.Cross, listOf(Color(0xFFFFFFFF), Color(0xFFFF0000))),

        // Detailed flags: drawn with dedicated painters, not stripe fallbacks.
        "US" to FlagStripeData(StripeStyle.UsStars, listOf(Color(0xFFB22234), Color(0xFFFFFFFF), Color(0xFF3C3B6E))),
        "GB" to FlagStripeData(StripeStyle.UnionJack, listOf(Color(0xFF012169), Color(0xFFFFFFFF), Color(0xFFC8102E))),
        "JP" to FlagStripeData(StripeStyle.Disc, listOf(Color(0xFFFFFFFF), Color(0xFFBC002D))),
        "BD" to FlagStripeData(StripeStyle.Disc, listOf(Color(0xFF006A4E), Color(0xFFF42A41))),
        "CN" to FlagStripeData(StripeStyle.CnStars, listOf(Color(0xFFDE2910), Color(0xFFFFDE00))),
        "TW" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF000095), Color(0xFFFE0000), Color(0xFFFE0000))),
        "HK" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFDE2910), Color(0xFFFFFFFF), Color(0xFFDE2910))),
        "KR" to FlagStripeData(StripeStyle.Taegeuk, listOf(Color(0xFFFFFFFF), Color(0xFFCD2E3A), Color(0xFF003478))),
        "TR" to FlagStripeData(StripeStyle.Crescent, listOf(Color(0xFFE30A17), Color(0xFFFFFFFF))),
        "TN" to FlagStripeData(StripeStyle.Crescent, listOf(Color(0xFFE70013), Color(0xFFFFFFFF))),
        "IL" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF0038B8), Color(0xFFFFFFFF))),
        "BR" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF009C3B), Color(0xFFFFDF00), Color(0xFF009C3B))),
        "AU" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF012169), Color(0xFFFFFFFF), Color(0xFF012169))),
        "NZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF00247D), Color(0xFFCC142B), Color(0xFF00247D))),
        "ZA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF007749), Color(0xFFFFB81C), Color(0xFFDE3831))),
        "KE" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF000000), Color(0xFFBB0000), Color(0xFF006600))),
        "KZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF00AFCA), Color(0xFFFFD700), Color(0xFF00AFCA))),
        "UZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF1EB53A), Color(0xFFFFFFFF), Color(0xFF0099B5))),
        "VN" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFDA251D), Color(0xFFFFCD00), Color(0xFFDA251D))),
        "SA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF006C35), Color(0xFFFFFFFF), Color(0xFF006C35))),
        "AE" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF00732F), Color(0xFFFFFFFF), Color(0xFF000000))),
        "QA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF8A1538), Color(0xFFFFFFFF), Color(0xFF8A1538))),
        "MY" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF010066), Color(0xFFCC0001), Color(0xFFFFFFFF))),
        "TH" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFA51931), Color(0xFFF4F5F8), Color(0xFF2D2A4A))),
        "PH" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF0038A8), Color(0xFFFFFFFF), Color(0xFFCE1126))),
        "MM" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFECB00), Color(0xFF34B233), Color(0xFFEA2839))),
        "BD" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF006A4E), Color(0xFFF42A41), Color(0xFF006A4E))),
        "PK" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF01411C), Color(0xFFFFFFFF), Color(0xFF01411C))),
        "CL" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF0039A6), Color(0xFFD52B1E))),
        "PA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFFDA121A), Color(0xFF003DA5))),
        "CU" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF002A8F), Color(0xFFFFFFFF), Color(0xFFCF142B))),
        "ES" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFAA151B), Color(0xFFF1BF00), Color(0xFFAA151B))),
        "CZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF11457E), Color(0xFFD7141A))),
        "SK" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFFFFFFF), Color(0xFF0B4EA2), Color(0xFFEE1C25))),
        "BA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF002395), Color(0xFFFECB00), Color(0xFF002395))),
        "AL" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFE41E20), Color(0xFF000000), Color(0xFFE41E20))),
        "MK" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFD20000), Color(0xFFFFE600), Color(0xFFD20000))),
        "ME" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFC40308), Color(0xFFD4AF37), Color(0xFFC40308))),
        "MA" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFC1272D), Color(0xFF006233), Color(0xFFC1272D))),
        "TN" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFFE70013), Color(0xFFFFFFFF), Color(0xFFE70013))),
        "DZ" to FlagStripeData(StripeStyle.Horizontal, listOf(Color(0xFF006233), Color(0xFFFFFFFF), Color(0xFFD21034)))
    )

    private val VALID_CODES: Set<String> = STRIPES.keys

    // Sorted longest-first name matches
    private val NAME_PAIRS: List<Pair<String, String>> = listOf(
        "united states" to "US", "usa" to "US", "america" to "US",
        "russia" to "RU", "germany" to "DE", "france" to "FR",
        "united kingdom" to "GB", "great britain" to "GB", "england" to "GB",
        "japan" to "JP", "china" to "CN", "south korea" to "KR", "korea" to "KR",
        "taiwan" to "TW", "hong kong" to "HK", "singapore" to "SG",
        "netherlands" to "NL", "holland" to "NL",
        "italy" to "IT", "spain" to "ES", "portugal" to "PT",
        "ireland" to "IE", "belgium" to "BE", "austria" to "AT",
        "switzerland" to "CH", "poland" to "PL", "czech" to "CZ", "czechia" to "CZ",
        "slovakia" to "SK", "hungary" to "HU", "romania" to "RO", "bulgaria" to "BG",
        "croatia" to "HR", "serbia" to "RS", "greece" to "GR", "turkey" to "TR",
        "turkiye" to "TR", "ukraine" to "UA", "moldova" to "MD", "georgia" to "GE",
        "armenia" to "AM", "azerbaijan" to "AZ", "kazakhstan" to "KZ",
        "uzbekistan" to "UZ", "sweden" to "SE", "norway" to "NO",
        "finland" to "FI", "denmark" to "DK", "iceland" to "IS",
        "estonia" to "EE", "latvia" to "LV", "lithuania" to "LT",
        "luxembourg" to "LU", "albania" to "AL", "north macedonia" to "MK",
        "bosnia" to "BA", "montenegro" to "ME", "slovenia" to "SI",
        "israel" to "IL", "india" to "IN", "thailand" to "TH",
        "vietnam" to "VN", "indonesia" to "ID", "malaysia" to "MY",
        "philippines" to "PH", "brazil" to "BR", "argentina" to "AR",
        "chile" to "CL", "colombia" to "CO", "mexico" to "MX",
        "canada" to "CA", "australia" to "AU", "new zealand" to "NZ",
        "south africa" to "ZA", "nigeria" to "NG", "kenya" to "KE",
        "egypt" to "EG", "uae" to "AE", "emirates" to "AE",
        "saudi arabia" to "SA", "panama" to "PA", "iran" to "IR",
        "iraq" to "IQ", "pakistan" to "PK", "bangladesh" to "BD",
        "cambodia" to "KH", "myanmar" to "MM", "mongolia" to "MN",
        // Russian country names
        "россия" to "RU", "германия" to "DE", "франция" to "FR",
        "великобритания" to "GB", "англия" to "GB", "япония" to "JP",
        "китай" to "CN", "корея" to "KR", "тайвань" to "TW",
        "гонконг" to "HK", "сингапур" to "SG", "нидерланды" to "NL",
        "голландия" to "NL", "италия" to "IT", "испания" to "ES",
        "португалия" to "PT", "ирландия" to "IE", "бельгия" to "BE",
        "австрия" to "AT", "швейцария" to "CH", "польша" to "PL",
        "чехия" to "CZ", "словакия" to "SK", "венгрия" to "HU",
        "румыния" to "RO", "болгария" to "BG", "хорватия" to "HR",
        "сербия" to "RS", "греция" to "GR", "турция" to "TR",
        "украина" to "UA", "молдова" to "MD", "молдавия" to "MD",
        "грузия" to "GE", "армения" to "AM", "азербайджан" to "AZ",
        "казахстан" to "KZ", "узбекистан" to "UZ", "швеция" to "SE",
        "норвегия" to "NO", "финляндия" to "FI", "дания" to "DK",
        "исландия" to "IS", "эстония" to "EE", "латвия" to "LV",
        "литва" to "LT", "люксембург" to "LU", "албания" to "AL",
        "македония" to "MK", "босния" to "BA", "черногория" to "ME",
        "словения" to "SI", "израиль" to "IL", "индия" to "IN",
        "таиланд" to "TH", "вьетнам" to "VN", "индонезия" to "ID",
        "малайзия" to "MY", "филиппины" to "PH", "бразилия" to "BR",
        "аргентина" to "AR", "чили" to "CL", "колумбия" to "CO",
        "мексика" to "MX", "канада" to "CA", "австралия" to "AU",
        "новая зеландия" to "NZ", "юар" to "ZA", "нигерия" to "NG",
        "кения" to "KE", "египет" to "EG", "оаэ" to "AE", "эмираты" to "AE",
        "саудовская аравия" to "SA", "панама" to "PA", "иран" to "IR",
        "ирак" to "IQ", "пакистан" to "PK", "монголия" to "MN",
        // Major cities
        "moscow" to "RU", "saint petersburg" to "RU", "novosibirsk" to "RU",
        "москва" to "RU", "питер" to "RU", "спб" to "RU", "петербург" to "RU",
        "new york" to "US", "los angeles" to "US", "chicago" to "US",
        "san francisco" to "US", "seattle" to "US", "dallas" to "US",
        "miami" to "US", "atlanta" to "US", "washington" to "US",
        "silicon valley" to "US", "ashburn" to "US", "phoenix" to "US",
        "las vegas" to "US", "denver" to "US", "boston" to "US",
        "berlin" to "DE", "frankfurt" to "DE", "munich" to "DE", "hamburg" to "DE",
        "dusseldorf" to "DE", "nuremberg" to "DE",
        "paris" to "FR", "marseille" to "FR", "lyon" to "FR",
        "london" to "GB", "manchester" to "GB", "edinburgh" to "GB",
        "tokyo" to "JP", "osaka" to "JP",
        "beijing" to "CN", "shanghai" to "CN", "guangzhou" to "CN", "shenzhen" to "CN",
        "seoul" to "KR", "busan" to "KR",
        "taipei" to "TW",
        "amsterdam" to "NL", "rotterdam" to "NL",
        "rome" to "IT", "milan" to "IT",
        "madrid" to "ES", "barcelona" to "ES",
        "lisbon" to "PT", "dublin" to "IE", "brussels" to "BE",
        "vienna" to "AT", "wien" to "AT",
        "zurich" to "CH", "geneva" to "CH", "bern" to "CH",
        "warsaw" to "PL", "krakow" to "PL",
        "prague" to "CZ", "budapest" to "HU", "bucharest" to "RO",
        "sofia" to "BG", "zagreb" to "HR", "belgrade" to "RS",
        "athens" to "GR",
        "kyiv" to "UA", "kiev" to "UA", "odessa" to "UA",
        "киев" to "UA", "одесса" to "UA",
        "tbilisi" to "GE", "yerevan" to "AM", "baku" to "AZ",
        "astana" to "KZ", "almaty" to "KZ", "tashkent" to "UZ",
        "stockholm" to "SE", "oslo" to "NO", "helsinki" to "FI",
        "copenhagen" to "DK", "reykjavik" to "IS",
        "tallinn" to "EE", "riga" to "LV", "vilnius" to "LT",
        "istanbul" to "TR", "ankara" to "TR",
        "tel aviv" to "IL", "jerusalem" to "IL",
        "dubai" to "AE", "abu dhabi" to "AE",
        "riyadh" to "SA", "jeddah" to "SA",
        "cairo" to "EG", "johannesburg" to "ZA", "cape town" to "ZA",
        "lagos" to "NG", "nairobi" to "KE",
        "sao paulo" to "BR", "rio de janeiro" to "BR",
        "buenos aires" to "AR", "santiago" to "CL", "bogota" to "CO",
        "mexico city" to "MX",
        "toronto" to "CA", "montreal" to "CA", "vancouver" to "CA",
        "sydney" to "AU", "melbourne" to "AU", "perth" to "AU",
        "auckland" to "NZ",
        "mumbai" to "IN", "delhi" to "IN", "bangalore" to "IN",
        "bangkok" to "TH",
        "ho chi minh" to "VN", "hanoi" to "VN",
        "jakarta" to "ID", "manila" to "PH", "kuala lumpur" to "MY"
    ).sortedByDescending { it.first.length }

    private val COUNTRY_TLDS: Set<String> = setOf(
        "ru", "de", "fr", "jp", "kr", "cn", "ua", "pl", "cz", "nl",
        "se", "no", "fi", "dk", "at", "ch", "it", "es", "pt", "ie",
        "be", "hu", "ro", "bg", "hr", "rs", "gr", "tr", "il", "br",
        "ar", "mx", "ca", "au", "nz", "in", "sg", "my", "th", "vn",
        "id", "ph", "eg", "za", "ng", "ke", "kz", "uz", "ge", "am",
        "az", "ee", "lv", "lt", "lu", "al", "mk", "ba", "sk", "md",
        "tw", "hk", "pk", "bd", "ir", "iq", "sa", "ae", "qa", "us",
        "uk"
    )

    private val TLD_REMAP: Map<String, String> = mapOf("uk" to "GB")

    fun getFlagEmoji(code: String?): String {
        val trimmed = code?.trim()?.uppercase(Locale.US) ?: return ""
        if (trimmed.length != 2 || !trimmed.all { it in 'A'..'Z' }) return ""
        val char1 = Character.toChars(0x1F1E6 + (trimmed[0] - 'A'))
        val char2 = Character.toChars(0x1F1E6 + (trimmed[1] - 'A'))
        return String(char1) + String(char2)
    }

    fun detectCountry(name: String?, server: String?): String {
        val safeName = name ?: ""
        val safeServer = server ?: ""

        val combined = "$safeName $safeServer".lowercase(Locale.ROOT)
        if (combined.contains("warp") || combined.contains("cloudflare")) {
            return "US"
        }

        val emojiResult = detectEmoji(safeName)
        if (emojiResult.isNotEmpty()) return emojiResult

        val nameResult = detectName(safeName)
        if (nameResult.isNotEmpty()) return nameResult

        val codeResult = detectCode(safeName)
        if (codeResult.isNotEmpty()) return codeResult

        val serverResult = detectServer(safeServer)
        if (serverResult.isNotEmpty()) return serverResult

        val serverCodeResult = detectCode(safeServer)
        if (serverCodeResult.isNotEmpty()) return serverCodeResult

        return ""
    }

    fun detectEmoji(text: String): String {
        var i = 0
        while (i < text.length - 1) {
            val cp1 = text.codePointAt(i)
            val charCount1 = Character.charCount(cp1)
            if (i + charCount1 < text.length) {
                val cp2 = text.codePointAt(i + charCount1)
                if (cp1 in 0x1F1E6..0x1F1FF && cp2 in 0x1F1E6..0x1F1FF) {
                    val c1 = ('A'.code + (cp1 - 0x1F1E6)).toChar()
                    val c2 = ('A'.code + (cp2 - 0x1F1E6)).toChar()
                    return "$c1$c2"
                }
            }
            i += charCount1
        }
        return ""
    }

    fun detectName(name: String): String {
        val lower = name.lowercase(Locale.ROOT)
        for ((pattern, code) in NAME_PAIRS) {
            val regex = Pattern.compile("\\b" + Pattern.quote(pattern) + "\\b", Pattern.UNICODE_CHARACTER_CLASS or Pattern.CASE_INSENSITIVE)
            if (regex.matcher(lower).find()) {
                return code
            }
        }
        return ""
    }

    fun detectCode(name: String): String {
        val uppercase = name.uppercase(Locale.ROOT)
        val regex = Pattern.compile("(?:^|[^A-Z])([A-Z]{2})(?:[^A-Z]|$)")
        val matcher = regex.matcher(uppercase)
        val ignoreWords = setOf("WG", "IP", "ID", "IN", "OR", "NO", "ON", "TO", "IT", "IS", "AM", "AT", "BY", "IF", "MY", "SO", "UP", "AN", "AS", "BE", "DO", "HE", "ME")
        while (matcher.find()) {
            val code = matcher.group(1) ?: ""
            if (VALID_CODES.contains(code) && !ignoreWords.contains(code)) {
                return code
            }
        }
        return ""
    }

    fun detectServer(server: String): String {
        if (server.isBlank()) return ""
        val cleanServer = server.lowercase(Locale.ROOT).trim('.')
        val parts = cleanServer.split('.')

        // TLD check (e.g. "vpn.company.ru" -> "ru", "relay.server.uk" -> "uk" -> "GB")
        val tld = parts.lastOrNull() ?: ""
        if (tld.length == 2 && COUNTRY_TLDS.contains(tld)) {
            return TLD_REMAP[tld] ?: tld.uppercase(Locale.ROOT)
        }

        // First label check (e.g. "de1.server.com" -> "de", "us.vpn-node.net" -> "us")
        val first = parts.firstOrNull() ?: ""
        val match = Regex("^([a-z]{2})(?:\\d|[-_]|$)").find(first)
        if (match != null) {
            val code = match.groupValues[1]
            if (COUNTRY_TLDS.contains(code)) {
                return TLD_REMAP[code] ?: code.uppercase(Locale.ROOT)
            }
        }

        return ""
    }


}
