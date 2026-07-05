module dut;
  function int get_data();
    return 42;
  endfunction

  task run_test();
    int x;
    x = get_data();
  endtask
endmodule
