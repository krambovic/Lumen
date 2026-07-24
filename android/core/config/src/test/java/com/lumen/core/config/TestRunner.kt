package com.lumen.core.config

import java.lang.reflect.Method
import org.junit.Test

fun main() {
    val testClasses = listOf(
        AmneziaWGNormalizerTest::class.java,
        ConfigBuilderTest::class.java
    )

    var total = 0
    var passed = 0
    var failed = 0

    println("==================================================")
    println("Running Unit Tests for :core:config")
    println("==================================================")

    for (cls in testClasses) {
        val instance = cls.getDeclaredConstructor().newInstance()
        val methods = cls.declaredMethods.filter { it.isAnnotationPresent(Test::class.java) }
        for (m in methods) {
            total++
            print("Running ${cls.simpleName}.${m.name} ... ")
            try {
                m.invoke(instance)
                passed++
                println("PASSED")
            } catch (e: Exception) {
                failed++
                println("FAILED: ${e.cause?.message ?: e.message}")
                e.cause?.printStackTrace()
            }
        }
    }

    println("==================================================")
    println("Results: $passed passed, $failed failed out of $total tests")
    println("==================================================")

    if (failed > 0) {
        kotlin.system.exitProcess(1)
    }
}
