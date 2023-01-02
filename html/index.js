const apply_operation = (op, a, b) => (
    {
      "+": (a, b) => a + b,
      "-": (a, b) => a - b,
      "*": (a, b) => a * b,
      "/": (a, b) => a / b,
    }[op](a,b)
  )

  function showResult(result) {
    const tmpl = document.getElementById("result-template")

    const result_node = document.querySelector("#results")
    result_node.innerHTML = ""

    if (result === null || typeof result === "undefined") {
      result_node.appendChild(document.getElementById("no-result").content.cloneNode(true))
      return
    }

    for (let i=result.operations.length - 1; i >= 0; i--) {
      const [op, a, b] = result.operations[i]
      const row = tmpl.content.cloneNode(true)

      const res = apply_operation(op, a, b)

      const nodes = [...row.querySelectorAll('[data-ev]')]

      nodes[0].innerText = a
      nodes[1].innerText = op
      nodes[2].innerText = b
      nodes[3].innerText = res

      result_node.appendChild(row)
    }
  }
  const pool = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
    25, 50, 75, 100
  ]

  const form = document.querySelector('form')
  const input_n = [...document.querySelectorAll('.base-numbers input[max="100"]')]
  const input_t = document.querySelector('#target')

  document.querySelector("#gen-random").addEventListener("click", () => {
    let mypool = [...pool]
    let numbers = []


    for (let i=0; i < 6; i++) {
      let idx = Math.random() * mypool.length
      //numbers.push(...mypool.splice(idx, 1))
      input_n[i].value = mypool.splice(idx, 1)[0]
    }
    input_t.value = parseInt(101 + Math.random() * 898, 10)
  })

  // Asynchronous import + init
  const solve_import = import( "./wasm/deschiffres.js").then((module) => module.default().then(() => module.solve_js))

  form.addEventListener('submit', (e) => {
      e.preventDefault()

      if (! form.checkValidity() ) {
        console.error("Form did not validate, abort")
        return
      }

      const numbers = input_n.map(x => parseInt(x.value, 10)).filter(x => !isNaN(x))
      const target = parseInt(input_t.value, 10)

      if (numbers.length != 6 || isNaN(target)) {
        console.error(`Need 6 numbers, got ${numbers.length}`)
        return
      }

      console.log(`Find ${target} with ${numbers}`)

      solve_import.then( (solve) => {
        console.time("Solving")
        let solved = solve(numbers, target, 0)
        showResult(solved)
        console.timeEnd("Solving")
        console.log("solved", solved)
      })
    })