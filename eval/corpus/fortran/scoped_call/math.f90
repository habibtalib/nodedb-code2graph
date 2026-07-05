module math_mod
contains
  function get_base() result(b)
    integer :: b
    b = 41
  end function get_base

  function compute() result(c)
    integer :: c
    c = get_base() + 1
  end function compute
end module math_mod
